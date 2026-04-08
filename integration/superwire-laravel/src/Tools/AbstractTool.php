<?php

namespace Superwire\Laravel\Tools;

use Illuminate\Support\Str;
use Superwire\Laravel\Contracts\Tool;
use Superwire\Laravel\Schema\JsonSchemaBuilder;

abstract class AbstractTool implements Tool
{
    public static function name(): string
    {
        return Str::snake(class_basename(static::class));
    }

    public static function description(): string
    {
        return sprintf('Proxy tool for %s', static::class);
    }

    public static function endpointName(): string
    {
        return static::name();
    }

    public static function inputSchema(): array
    {
        return JsonSchemaBuilder::object()->toArray();
    }

    public static function boundInputSchema(): array
    {
        return JsonSchemaBuilder::object()->toArray();
    }

    public static function outputSchema(): array
    {
        return JsonSchemaBuilder::object()->toArray();
    }
}
