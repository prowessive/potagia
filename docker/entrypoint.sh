#!/bin/sh
set -eu

readonly source_password="/run/secrets/postgres_password"
readonly runtime_password="/tmp/potagia_postgres_password"

if [ "$(id -u)" -eq 0 ]; then
    install -m 0400 "$source_password" "$runtime_password"
    chown 10001:10001 "$runtime_password"
    export POSTGRES_PASSWORD_FILE="$runtime_password"
    exec gosu 10001:10001 "$@"
fi

exec "$@"
