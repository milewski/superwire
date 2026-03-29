#!/usr/bin/env bash

set -euo pipefail

project_directory="/workspace/crates/ffi/php"

if [[ ! -d "${project_directory}" ]]; then
    echo "Project directory ${project_directory} was not found."
    exec "$@"
fi

cd "${project_directory}"

echo "Installing PHP dependencies and building native extension..."
composer install --no-interaction --prefer-dist

php_version="$(php -r 'echo PHP_MAJOR_VERSION . "." . PHP_MINOR_VERSION;')"
debian_ini_directory="/etc/php/${php_version}/cli/conf.d"

if [[ -d "${debian_ini_directory}" ]]; then
    ini_file_path="${debian_ini_directory}/99-engine-ai-ffi.ini"
else
    mkdir -p /usr/local/etc/php/conf.d
    ini_file_path="/usr/local/etc/php/conf.d/99-engine-ai-ffi.ini"
fi

printf '%s\n' 'extension=engine_ai_ffi' > "${ini_file_path}"

echo "engine_ai_ffi enabled in ${ini_file_path}"
echo "Container ready."

exec "$@"
