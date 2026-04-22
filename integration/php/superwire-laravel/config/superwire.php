<?php

declare(strict_types = 1);

return [
    'runtime' => [
        'fork' => env('SUPERWIRE_RUNTIME_FORK', true),
        'stream' => env('SUPERWIRE_RUNTIME_STREAM', true),
        'max_agent_request_attempts' => env('SUPERWIRE_MAX_AGENT_REQUEST_ATTEMPTS', 10),
        'max_agent_tool_steps' => env('SUPERWIRE_MAX_AGENT_TOOL_STEPS', 20),
    ],

    'cli' => [
        'path' => env('SUPERWIRE_CLI_PATH', base_path('superwire-cli')),
    ],

    'prism' => [
        'providers' => [],
    ],
];
