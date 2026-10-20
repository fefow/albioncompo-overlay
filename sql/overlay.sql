-- RPCs del overlay de Windows (las llama con la anon key).
--
--   POST /rest/v1/rpc/get_overlay   { "p_code": "D75TGV", "p_rev": "<rev anterior o null>" }
--   POST /rest/v1/rpc/join_overlay  { "p_code": "D75TGV", "p_name": "Dusius", "p_role": "HEALER" }
--
-- Flujo: el creador arma la asignación en la web; el jugador pone el código en
-- el overlay, se anota con su nombre y rol (join_overlay), y el overlay hace
-- polling de get_overlay hasta que el creador le asigna una build en la web.
-- El overlay recuerda su nombre y se encuentra en "entries" por nombre.
--
-- Respuestas (siempre jsonb con "status"):
--   get_overlay:  ok | unchanged | expired | invalid | rate_limited
--   join_overlay: ok | name_taken | full | bad_name | expired | invalid | rate_limited
--
-- Idempotente: se puede volver a ejecutar entero sin romper nada.

-- 1. Registro de intentos, para frenar a quien pruebe códigos al azar
--    (kind = 'fail') o llene una asignación de nombres basura (kind = 'join').
--    Sin políticas RLS: solo las funciones (SECURITY DEFINER) la tocan.
create table if not exists public.overlay_attempts (
  id bigint generated always as identity primary key,
  kind text not null check (kind in ('fail', 'join')),
  client_ip text not null,
  at timestamptz not null default now()
);

create index if not exists overlay_attempts_kind_ip_at_idx
  on public.overlay_attempts (kind, client_ip, at desc);

create index if not exists overlay_attempts_kind_at_idx
  on public.overlay_attempts (kind, at desc);

alter table public.overlay_attempts enable row level security;
revoke all on public.overlay_attempts from anon, authenticated;

-- 2. Búsqueda por hash: hoy no hay índice y el overlay va a consultar seguido.
create index if not exists composition_assignments_access_code_hash_idx
  on public.composition_assignments (access_code_hash, expires_at desc);

create index if not exists composition_assignment_entries_assignment_id_idx
  on public.composition_assignment_entries (assignment_id);

-- 3. Helpers internos (no se exponen a anon).

-- IP del cliente. cf-connecting-ip lo pone Cloudflare (delante de Supabase)
-- y el cliente no lo puede falsificar; x-forwarded-for queda de respaldo.
create or replace function public.overlay_client_ip()
returns text
language sql
stable
set search_path = ''
as $$
  select coalesce(
    nullif(btrim(h ->> 'cf-connecting-ip'), ''),
    nullif(btrim(split_part(h ->> 'x-forwarded-for', ',', 1)), ''),
    'unknown'
  )
  from (select nullif(current_setting('request.headers', true), '')::json as h) s
$$;

-- true si la IP (o todo el sistema) pasó el límite de ese tipo de intento.
create or replace function public.overlay_over_limit(p_kind text, p_ip text, p_ip_max int, p_all_max int)
returns boolean
language sql
stable
set search_path = ''
as $$
  select (select count(*) from public.overlay_attempts
           where kind = p_kind and client_ip = p_ip and at > now() - interval '10 minutes') >= p_ip_max
      or (select count(*) from public.overlay_attempts
           where kind = p_kind and at > now() - interval '10 minutes') >= p_all_max
$$;

create or replace function public.overlay_log_attempt(p_kind text, p_ip text)
returns void
language sql
volatile
set search_path = ''
as $$
  insert into public.overlay_attempts (kind, client_ip) values (p_kind, p_ip);
  -- Limpieza barata: solo se guarda la última hora.
  delete from public.overlay_attempts where at < now() - interval '1 hour';
$$;

-- Normaliza el código y devuelve la asignación más reciente con ese hash.
-- Mismo alfabeto que generateAccessCode() y mismo hash que hashAccessCode()
-- de la web (sha256 hex). Fila vacía si el formato es inválido o no existe.
create or replace function public.overlay_find_assignment(p_code text)
returns public.composition_assignments
language sql
stable
set search_path = ''
as $$
  select a.*
    from public.composition_assignments a
   where upper(btrim(coalesce(p_code, ''))) ~ '^[A-HJ-NP-Z2-9]{6}$'
     and a.access_code_hash = encode(sha256(convert_to(upper(btrim(p_code)), 'UTF8')), 'hex')
   order by a.expires_at desc
   limit 1
$$;

revoke all on function public.overlay_client_ip() from public, anon, authenticated;
revoke all on function public.overlay_over_limit(text, text, int, int) from public, anon, authenticated;
revoke all on function public.overlay_log_attempt(text, text) from public, anon, authenticated;
revoke all on function public.overlay_find_assignment(text) from public, anon, authenticated;

-- 4. get_overlay: lo que dibuja el overlay.
create or replace function public.get_overlay(p_code text, p_rev text default null)
returns jsonb
language plpgsql
volatile
security definer
set search_path = ''
as $$
declare
  v_ip      text := public.overlay_client_ip();
  v_assign  public.composition_assignments;
  v_comp    record;
  v_payload jsonb;
  v_rev     text;
begin
  -- Un código mal formado no puede existir: se responde sin contar intento.
  if upper(btrim(coalesce(p_code, ''))) !~ '^[A-HJ-NP-Z2-9]{6}$' then
    return jsonb_build_object('status', 'invalid');
  end if;

  -- 10 fallos por IP cada 10 min, y 300 en total como freno si rotan IPs.
  -- Con ~10 asignaciones activas, eso da ~0,04 % por día de acertar una.
  if public.overlay_over_limit('fail', v_ip, 10, 300) then
    return jsonb_build_object('status', 'rate_limited', 'retry_after', 600);
  end if;

  v_assign := public.overlay_find_assignment(p_code);

  if v_assign.id is null then
    perform public.overlay_log_attempt('fail', v_ip);
    return jsonb_build_object('status', 'invalid');
  end if;

  if v_assign.expires_at <= now() then
    return jsonb_build_object('status', 'expired', 'expires_at', v_assign.expires_at);
  end if;

  select c.name, c.data
    into v_comp
    from public.compositions c
   where c.share_id = v_assign.share_id;

  if not found then
    return jsonb_build_object('status', 'invalid');
  end if;

  -- Solo lo que dibuja el overlay: nada de owner_id, notas, hashes ni ids internos.
  v_payload := jsonb_build_object(
    'assignment', jsonb_build_object('name', v_assign.assignment_name),
    'composition', jsonb_build_object('name', v_comp.name),
    'entries', coalesce((
      select jsonb_agg(jsonb_build_object(
               'name', e.name,
               'role', e.role,
               'build_id', e.build_id
             ) order by e.name_norm)
        from public.composition_assignment_entries e
       where e.assignment_id = v_assign.id
    ), '[]'::jsonb),
    'builds', coalesce((
      select jsonb_agg(jsonb_build_object(
               'id', b ->> '_id',
               'name', b ->> 'name',
               'role', b ->> 'role',
               'weapon', b ->> 'weapon',
               'secondhand', b ->> 'secondhand',
               'helm', b ->> 'helm',
               'armor', b ->> 'armor',
               'boot', b ->> 'boot',
               'cape', b ->> 'cape',
               'potion', b ->> 'potion',
               'food', b ->> 'food',
               'mount', b ->> 'mount',
               'variants', coalesce(b -> 'variants', '{}'::jsonb)
             ) order by ord)
        from jsonb_array_elements(
               case when jsonb_typeof(v_comp.data) = 'array' then v_comp.data else '[]'::jsonb end
             ) with ordinality as t(b, ord)
    ), '[]'::jsonb)
  );

  v_rev := md5(v_payload::text);

  if p_rev is not null and p_rev = v_rev then
    return jsonb_build_object('status', 'unchanged', 'rev', v_rev);
  end if;

  return v_payload || jsonb_build_object(
    'status', 'ok',
    'rev', v_rev,
    'expires_at', v_assign.expires_at
  );
end;
$$;

-- 5. join_overlay: el jugador se anota desde el overlay.
--    Hace lo mismo que POST /api/compositions/[shareId]/assignments/[assignId]/entries
--    pero validando con el código en vez de la cookie. La build la asigna el
--    creador desde la web, como hoy.
create or replace function public.join_overlay(p_code text, p_name text, p_role text default null)
returns jsonb
language plpgsql
volatile
security definer
set search_path = ''
as $$
declare
  v_ip     text := public.overlay_client_ip();
  v_assign public.composition_assignments;
  -- Igual que normalizeName() de la web.
  v_name   text := btrim(regexp_replace(coalesce(p_name, ''), '\s+', ' ', 'g'));
  v_norm   text := lower(btrim(regexp_replace(coalesce(p_name, ''), '\s+', ' ', 'g')));
  v_role   text := nullif(left(btrim(coalesce(p_role, '')), 32), '');
begin
  if upper(btrim(coalesce(p_code, ''))) !~ '^[A-HJ-NP-Z2-9]{6}$' then
    return jsonb_build_object('status', 'invalid');
  end if;

  -- Los nombres de Albion son de 3 a 16 caracteres; se deja margen.
  if char_length(v_name) < 2 or char_length(v_name) > 32 then
    return jsonb_build_object('status', 'bad_name');
  end if;

  if public.overlay_over_limit('fail', v_ip, 10, 300)
     or public.overlay_over_limit('join', v_ip, 5, 500) then
    return jsonb_build_object('status', 'rate_limited', 'retry_after', 600);
  end if;

  v_assign := public.overlay_find_assignment(p_code);

  if v_assign.id is null then
    perform public.overlay_log_attempt('fail', v_ip);
    return jsonb_build_object('status', 'invalid');
  end if;

  if v_assign.expires_at <= now() then
    return jsonb_build_object('status', 'expired', 'expires_at', v_assign.expires_at);
  end if;

  -- Tope por asignación para que nadie la llene de basura.
  if (select count(*) from public.composition_assignment_entries
       where assignment_id = v_assign.id) >= 200 then
    return jsonb_build_object('status', 'full');
  end if;

  if exists (select 1 from public.composition_assignment_entries
              where assignment_id = v_assign.id and name_norm = v_norm) then
    return jsonb_build_object('status', 'name_taken');
  end if;

  insert into public.composition_assignment_entries
    (assignment_id, build_id, name, name_norm, role, code_hash)
  values
    (v_assign.id, null, v_name, v_norm, v_role, v_assign.access_code_hash);

  perform public.overlay_log_attempt('join', v_ip);

  -- Avisa a la página del creador por el mismo canal que usa la web
  -- (assignment-sync:<assignId>). Si Realtime no está disponible, el
  -- jugador queda anotado igual y el creador lo ve al recargar.
  begin
    perform realtime.send(
      jsonb_build_object('assignmentId', v_assign.id::text, 'reason', 'entry-added',
                         'at', floor(extract(epoch from now()) * 1000)),
      'assignment-sync',
      'assignment-sync:' || v_assign.id::text,
      false
    );
  exception when others then
    null;
  end;

  return jsonb_build_object('status', 'ok', 'name', v_name, 'role', v_role);
end;
$$;

revoke all on function public.get_overlay(text, text) from public;
revoke all on function public.join_overlay(text, text, text) from public;
grant execute on function public.get_overlay(text, text) to anon, authenticated;
grant execute on function public.join_overlay(text, text, text) to anon, authenticated;
