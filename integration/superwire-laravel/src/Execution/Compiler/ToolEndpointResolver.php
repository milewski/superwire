<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Execution\Compiler;

use Illuminate\Contracts\Config\Repository;

final readonly class ToolEndpointResolver
{
    public function __construct(private Repository $config)
    {
    }

    public function resolve(string $endpointName): string
    {
        $baseUrl = rtrim((string) $this->config->get('superwire.tools.http_endpoint_base_url', 'http://127.0.0.1:8000'), '/');
        $prefix = trim((string) $this->config->get('superwire.tools.http_prefix', 'superwire/tools'), '/');

        return sprintf('%s/%s/%s/execute', $baseUrl, $prefix, $endpointName);
    }
}
