#!/usr/bin/env bash
# Dev database lifecycle for one git worktree.
#
# Every worktree shares a single postgres container but gets its own database,
# named after the worktree directory. New databases are forked from a source
# database (the main worktree's, unless DEV_PG_SOURCE says otherwise) so a fresh
# worktree starts with the same data instead of an empty schema.
set -euo pipefail

backend_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

container="${DEV_PG_CONTAINER:-zet-live-pg}"
image="${DEV_PG_IMAGE:-postgres:18}"
host="${DEV_PG_HOST:-localhost}"
port="${DEV_PG_PORT:-5436}"
user="${DEV_PG_USER:-zet_live}"
password="${DEV_PG_PASSWORD:-zet_live}"
prefix="${DEV_PG_DB_PREFIX:-zet_live}"

die() {
  echo "dev-db: $*" >&2
  exit 1
}

# Runs SQL against the maintenance database, which exists even when the database
# being created or dropped does not. Statements arrive on stdin because psql
# only interpolates variables there, not in -c.
psql_maintenance() {
  docker exec -i "$container" psql -q -v ON_ERROR_STOP=1 -U "$user" -d postgres "$@"
}

query_maintenance() {
  psql_maintenance -t -A "$@"
}

db_exists() {
  local found
  found="$(
    query_maintenance -v target="$1" <<'SQL'
select 1 from pg_database where datname = :'target';
SQL
  )"
  [ -n "$found" ]
}

connection_count() {
  query_maintenance -v target="$1" <<'SQL'
select count(*) from pg_stat_activity
where datname = :'target' and pid <> pg_backend_pid();
SQL
}

terminate_connections() {
  query_maintenance -v target="$1" <<'SQL'
select count(*) from (
  select pg_terminate_backend(pid) from pg_stat_activity
  where datname = :'target' and pid <> pg_backend_pid()
) terminated;
SQL
}

create_empty_db() {
  psql_maintenance -v target="$1" <<'SQL'
CREATE DATABASE :"target";
SQL
}

create_db_from() {
  psql_maintenance -v source="$1" -v target="$2" <<'SQL'
CREATE DATABASE :"target" TEMPLATE :"source";
SQL
}

drop_db() {
  drop_db_connections "$1"
  psql_maintenance -v target="$1" <<'SQL'
DROP DATABASE IF EXISTS :"target";
SQL
}

drop_db_connections() {
  local killed
  killed="$(terminate_connections "$1")"
  if [ "$killed" -ne 0 ]; then
    echo "dev-db: kicked $killed connection(s) from $1" >&2
  fi
}

migrate_db() {
  # The URL on the command line wins over backend/.env.
  DATABASE_URL="$(db_url "$1")" sqlx migrate run
}

ensure_up() {
  if [ "$(docker inspect --format '{{.State.Running}}' "$container" 2>/dev/null || true)" = "true" ]; then
    return
  fi

  if docker container inspect "$container" >/dev/null 2>&1; then
    docker start "$container" >/dev/null
  else
    docker run --detach \
      --name "$container" \
      -e POSTGRES_USER="$user" \
      -e POSTGRES_PASSWORD="$password" \
      -e POSTGRES_DB="$prefix" \
      -p "127.0.0.1:$port:5432" \
      "$image" >/dev/null
  fi

  for _ in $(seq 1 60); do
    if docker exec "$container" pg_isready -U "$user" -q; then
      return
    fi
    sleep 0.5
  done
  die "$container did not become ready"
}

worktree_root() {
  git -C "$backend_dir" rev-parse --show-toplevel
}

# Branch and path per worktree, tab-separated. Detached worktrees have no branch.
list_worktrees() {
  git -C "$backend_dir" worktree list --porcelain | awk '
    /^worktree / { path = substr($0, 10); next }
    /^branch / { branch = substr($0, 8); sub(/^refs\/heads\//, "", branch); print branch "\t" path; next }
    /^detached$/ { print "-\t" path }
  '
}

slug_for_path() {
  local root="$1" name clean hash
  name="$(basename -- "$root")"
  clean="$(printf '%s' "$name" |
    tr '[:upper:]' '[:lower:]' |
    tr -cs 'a-z0-9' '-' |
    tr -s '-' |
    sed -e 's/^-//' -e 's/-$//')"
  [ -n "$clean" ] || clean="wt"
  # Postgres truncates identifiers at 63 bytes, so the slug alone is not unique.
  # Hashing the full path keeps similarly named worktrees apart after truncation.
  hash="$(printf '%s' "$root" | sha256sum | cut -c1-6)"
  printf '%s-%s' "${clean:0:32}" "$hash"
}

db_name_for_path() {
  printf '%s_%s' "$prefix" "$(slug_for_path "$1")"
}

current_db() {
  db_name_for_path "$(worktree_root)"
}

# Throwaway database for `sqlx prepare`, which needs a migrated schema and
# nothing else, so the offline cache never touches real worktree data.
scratch_db() {
  printf '%s_scratch-%s' "$prefix" "$(worktree_root | sha256sum | cut -c1-6)"
}

db_url() {
  # The password is interpolated raw, so it must be URL-safe.
  printf 'postgres://%s:%s@%s:%s/%s' "$user" "$password" "$host" "$port" "$1"
}

resolve_db_name() {
  local query="${1:-}" branch path
  if [ -z "$query" ]; then
    current_db
    return
  fi

  while IFS=$'\t' read -r branch path; do
    if [ "$branch" = "$query" ] || [ "$(basename -- "$path")" = "$query" ]; then
      db_name_for_path "$path"
      return
    fi
  done < <(list_worktrees)

  if db_exists "$query"; then
    printf '%s' "$query"
    return
  fi

  die "no worktree or database named '$query' (see: just dev-db list)"
}

source_db() {
  local branch path
  if [ -n "${DEV_PG_SOURCE:-}" ]; then
    resolve_db_name "$DEV_PG_SOURCE"
    return
  fi

  while IFS=$'\t' read -r branch path; do
    if [ "$branch" = "main" ]; then
      db_name_for_path "$path"
      return
    fi
  done < <(list_worktrees)

  die "no worktree for branch 'main'; set DEV_PG_SOURCE to the database to fork from"
}

fork_db() {
  local source="$1" target="$2"
  # CREATE DATABASE ... TEMPLATE refuses to run while any session is connected
  # to the source, and a dev server in another worktree stays connected.
  drop_db_connections "$source"
  create_db_from "$source" "$target"
  echo "dev-db: forked $source into $target" >&2
}

# Creates target as a copy of source and migrates it, or says why it cannot. An
# empty database is only valid when this worktree is the source, which is how
# the source itself gets bootstrapped.
provision_db() {
  local target="$1" source="$2"

  if [ "$target" = "$source" ]; then
    echo "dev-db: $target is the source database, creating it from migrations only" >&2
    create_empty_db "$target"
  elif db_exists "$source"; then
    fork_db "$source" "$target"
  else
    die "source database $source does not exist; run 'just dev-db create' in the main worktree first"
  fi

  # A branch can be behind the source, and sqlx refuses to migrate a database
  # that recorded migrations the branch does not have. Drop what was just
  # created so a later retry starts clean.
  if ! migrate_db "$target"; then
    drop_db "$target"
    die "$target could not be migrated and was dropped; $source may carry migrations this branch does not have, so point DEV_PG_SOURCE at a database from a compatible branch"
  fi

  echo "dev-db: $target is ready at $(db_url "$target")"
}

cmd_up() {
  ensure_up
  echo "dev-db: $container is up on 127.0.0.1:$port"
}

cmd_down() {
  if docker container inspect "$container" >/dev/null 2>&1; then
    docker stop "$container" >/dev/null
    echo "dev-db: stopped $container"
  else
    echo "dev-db: there is no container named $container"
  fi
}

cmd_create() {
  local target source
  target="$(current_db)"
  source="$(source_db)"
  ensure_up

  if db_exists "$target"; then
    echo "dev-db: $target already exists, use 'just dev-db reset' to rebuild it"
    return
  fi

  provision_db "$target" "$source"
}

cmd_reset() {
  local target source
  target="$(current_db)"
  source="$(source_db)"
  ensure_up

  if db_exists "$target"; then
    if [ "$target" = "$source" ]; then
      echo "dev-db: $target is the source database, it will end up empty" >&2
    fi
    drop_db "$target"
  fi

  provision_db "$target" "$source"
}

cmd_remove() {
  local force=false query="" arg target source connections
  for arg in "$@"; do
    case "$arg" in
    --force) force=true ;;
    -*) die "unknown option $arg" ;;
    *) query="$arg" ;;
    esac
  done

  target="$(resolve_db_name "$query")"
  source="$(source_db)"
  if [ "$target" = "$source" ]; then
    die "$target is the source database; point DEV_PG_SOURCE elsewhere to remove it"
  fi

  ensure_up
  if ! db_exists "$target"; then
    die "database $target does not exist"
  fi

  connections="$(connection_count "$target")"
  if [ "$connections" -gt 0 ] && [ "$force" = false ]; then
    die "$target has $connections connection(s); stop the dev server or pass --force"
  fi

  drop_db "$target"
  echo "dev-db: removed $target"
}

cmd_list() {
  local scratch db owner branch path
  scratch="$(scratch_db)"
  ensure_up

  printf '%-40s %-8s %s\n' DATABASE STATE OWNER

  while IFS=$'\t' read -r branch path; do
    db="$(db_name_for_path "$path")"
    if db_exists "$db"; then
      if [ "$branch" = "-" ]; then branch="(detached)"; fi
      print_db_row "$db" "$branch ($path)"
    fi
  done < <(list_worktrees)

  # Databases no worktree maps to, so what a deleted worktree left behind stays visible.
  while read -r db; do
    if [ "$db" = "$scratch" ]; then
      owner="sqlx scratch"
    elif known_to_worktree "$db"; then
      continue
    else
      owner="no worktree"
    fi
    print_db_row "$db" "$owner"
  done < <(list_dev_databases)
}

known_to_worktree() {
  local branch path db
  while IFS=$'\t' read -r branch path; do
    db="$(db_name_for_path "$path")"
    if [ "$db" = "$1" ]; then
      return 0
    fi
  done < <(list_worktrees)
  return 1
}

print_db_row() {
  local connections
  connections="$(connection_count "$1")"
  if [ "$connections" -eq 0 ]; then
    printf '%-40s %-8s %s\n' "$1" idle "$2"
  else
    printf '%-40s %-8s %s\n' "$1" in-use "$2"
  fi
}

list_dev_databases() {
  query_maintenance -v prefix="$prefix" <<'SQL'
select datname from pg_database
where left(datname, length(:'prefix') + 1) = :'prefix' || '_'
order by datname;
SQL
}

cmd_migrate() {
  local db="${1:-}"
  ensure_up
  migrate_db "$(resolve_db_name "$db")"
}

cmd_reset_scratch() {
  local db
  db="$(scratch_db)"
  ensure_up

  if db_exists "$db"; then
    drop_db "$db"
  fi
  create_empty_db "$db"
  migrate_db "$db"
}

cmd_env() {
  local target="${XDG_CONFIG_HOME:-$HOME/.config}/zet-live/dev.env"

  if [ -L "$backend_dir/.env" ]; then
    echo "dev-env: backend/.env -> $(readlink "$backend_dir/.env")"
    return
  fi

  mkdir -p "$(dirname "$target")"

  if [ ! -e "$target" ]; then
    if [ -f "$backend_dir/.env" ]; then
      # The database URL is per worktree now, so drop any inherited value.
      grep -v '^DATABASE_URL' "$backend_dir/.env" >"$target"
      chmod 600 "$target"
      rm "$backend_dir/.env"
    elif [ -f "$backend_dir/.env.example" ]; then
      cp "$backend_dir/.env.example" "$target"
      chmod 600 "$target"
    else
      die "no backend/.env or backend/.env.example to seed $target from"
    fi
  elif [ -f "$backend_dir/.env" ]; then
    die "$target already exists, so backend/.env was not copied into it; move it yourself or delete it"
  fi

  ln -s "$target" "$backend_dir/.env"
  echo "dev-env: backend/.env -> $target"
}

cmd_name() {
  current_db
  echo
}

cmd_url() {
  db_url "$(resolve_db_name "${1:-}")"
  echo
}

cmd_scratch_url() {
  db_url "$(scratch_db)"
}

usage() {
  cat <<'USAGE'
usage: just dev-db <command> [args]

  up                   start the shared postgres container
  down                 stop it
  create               create this worktree's database, forked from the source
  reset                drop, fork, and migrate this worktree's database
  remove [branch|dir]  drop a worktree's database (--force kicks live sessions)
  list                 list dev databases and the worktrees that own them
  migrate [branch|dir] run migrations against a worktree's database
  name                 print this worktree's database name
  url [branch|dir]     print a worktree's DATABASE_URL
  env                  link backend/.env to a file shared by every worktree

env: DEV_PG_CONTAINER DEV_PG_IMAGE DEV_PG_HOST DEV_PG_PORT DEV_PG_USER
     DEV_PG_PASSWORD DEV_PG_DB_PREFIX DEV_PG_SOURCE
USAGE
}

main() {
  local command="${1:-}"
  if [ $# -gt 0 ]; then
    shift
  fi

  case "$command" in
  up) cmd_up "$@" ;;
  down) cmd_down "$@" ;;
  create) cmd_create "$@" ;;
  reset) cmd_reset "$@" ;;
  remove) cmd_remove "$@" ;;
  list) cmd_list "$@" ;;
  migrate) cmd_migrate "$@" ;;
  name) cmd_name "$@" ;;
  url) cmd_url "$@" ;;
  env) cmd_env "$@" ;;
  scratch-url) cmd_scratch_url "$@" ;;
  reset-scratch) cmd_reset_scratch "$@" ;;
  "" | -h | --help | help) usage ;;
  *)
    usage
    die "unknown command '$command'"
    ;;
  esac
}

main "$@"
