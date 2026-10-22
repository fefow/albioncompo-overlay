-- RPCs del buscador de compos del overlay (las llama con la anon key).
--
--   POST /rest/v1/rpc/list_public_comps  { "p_query": "zvz", "p_offset": 0 }
--   POST /rest/v1/rpc/get_comp           { "p_ref": "<share_id o código privado>" }
--
-- Respuestas (siempre jsonb con "status"):
--   list_public_comps: ok | rate_limited
--   get_comp:          ok | invalid | rate_limited
--
-- Solo se consulta cuando la persona busca o abre una compo: sin polling.
-- Requiere haber corrido antes sql/overlay.sql (usa sus helpers y su tabla de
-- intentos). Idempotente: se puede volver a ejecutar entero.

-- La tabla de intentos pasa a aceptar también las búsquedas ('comp').
alter table public.overlay_attempts drop constraint if exists overlay_attempts_kind_check;
alter table public.overlay_attempts
  add constraint overlay_attempts_kind_check check (kind in ('fail', 'join', 'comp'));

-- 1. Lista de compos públicas, con búsqueda por nombre. De a 12, las más
--    recientes primero; "more" avisa si hay más para pedir con p_offset.
create or replace function public.list_public_comps(p_query text default null, p_offset int default 0)
returns jsonb
language plpgsql
volatile
security definer
set search_path = ''
as $$
declare
  v_ip    text := public.overlay_client_ip();
  v_q     text := nullif(btrim(left(coalesce(p_query, ''), 40)), '');
  v_like  text;
  v_rows  jsonb;
begin
  if public.overlay_over_limit('comp', v_ip, 60, 3000) then
    return jsonb_build_object('status', 'rate_limited', 'retry_after', 600);
  end if;
  perform public.overlay_log_attempt('comp', v_ip);

  if v_q is not null then
    v_like := '%' || replace(replace(replace(v_q, '\', '\\'), '%', '\%'), '_', '\_') || '%';
  end if;

  select coalesce(jsonb_agg(jsonb_build_object(
           'share_id', c.share_id,
           'name', c.name,
           'activity', c.activity,
           'builds', case when jsonb_typeof(c.data::jsonb) = 'array'
                          then jsonb_array_length(c.data::jsonb) else 0 end
         )), '[]'::jsonb)
    into v_rows
    from (
      select share_id, name, activity, data
        from public.compositions
       where is_public
         and share_id is not null
         and (v_like is null or name ilike v_like escape '\')
       order by updated_at desc nulls last, created_at desc
      offset greatest(coalesce(p_offset, 0), 0)
      limit 13
    ) c;

  return jsonb_build_object(
    'status', 'ok',
    'items', (select coalesce(jsonb_agg(e order by n), '[]'::jsonb)
                from jsonb_array_elements(v_rows) with ordinality x(e, n)
               where n <= 12),
    'more', jsonb_array_length(v_rows) > 12
  );
end;
$$;

-- 2. Una compo con sus builds: pública por su share_id, o privada por su código
--    de acceso (el mismo que usa la web, sha256 hex). Un código que no existe
--    cuenta como intento fallido, igual que en get_overlay.
create or replace function public.get_comp(p_ref text)
returns jsonb
language plpgsql
volatile
security definer
set search_path = ''
as $$
declare
  v_ip      text := public.overlay_client_ip();
  v_ref     text := btrim(coalesce(p_ref, ''));
  v_comp    record;
  v_private boolean := false;
begin
  if v_ref !~ '^[A-Za-z0-9_-]{6,64}$' then
    return jsonb_build_object('status', 'invalid');
  end if;

  if public.overlay_over_limit('fail', v_ip, 10, 300) then
    return jsonb_build_object('status', 'rate_limited', 'retry_after', 600);
  end if;

  select c.name, c.data
    into v_comp
    from public.compositions c
   where c.share_id = v_ref and c.is_public;

  if not found and char_length(v_ref) = 6 then
    -- Código privado: como lo escribió la persona, y en mayúsculas por si acaso.
    select c.name, c.data
      into v_comp
      from public.compositions c
     where not c.is_public
       and c.access_code_hash in (
             encode(sha256(convert_to(v_ref, 'UTF8')), 'hex'),
             encode(sha256(convert_to(upper(v_ref), 'UTF8')), 'hex'))
     limit 1;
    v_private := found;
  end if;

  if not found then
    perform public.overlay_log_attempt('fail', v_ip);
    return jsonb_build_object('status', 'invalid');
  end if;

  return jsonb_build_object(
    'status', 'ok',
    'private', v_private,
    'composition', jsonb_build_object('name', v_comp.name),
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
               case when jsonb_typeof(v_comp.data::jsonb) = 'array'
                    then v_comp.data::jsonb else '[]'::jsonb end
             ) with ordinality as t(b, ord)
    ), '[]'::jsonb)
  );
end;
$$;

revoke all on function public.list_public_comps(text, int) from public;
revoke all on function public.get_comp(text) from public;
grant execute on function public.list_public_comps(text, int) to anon, authenticated;
grant execute on function public.get_comp(text) to anon, authenticated;
