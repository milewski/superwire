<?php

return [
    'cli' => [
        'binary' => env('SUPERWIRE_CLI_BINARY', 'cli'),
        'working_directory' => base_path(),
        'timeout_seconds' => 120,
    ],

    'build' => [
        'root_directory' => storage_path('app/superwire'),
        'tools_directory' => base_path('tools'),
    ],

    'runtime' => [
        'internal_token' => env('SUPERWIRE_INTERNAL_TOKEN'),
    ],

    'tools' => [
        'registered_classes' => [],
        'http_endpoint_base_url' => env('SUPERWIRE_INTERNAL_ENDPOINT_BASE_URL', 'http://127.0.0.1'),
        'http_prefix' => 'superwire/tools',
    ],

    'security' => [
        'enforce_localhost_only' => true,
    ],

    'routes' => [
        'middleware' => ['api'],
    ],
];
