<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use RuntimeException;
use Superwire\Laravel\Contracts\WitDefinedTool;
use Superwire\Laravel\Wit\WitToolSchemaParser;
use Superwire\Laravel\Wit\Schema\WitToolSchema;

abstract class AbstractWitTool extends AbstractTool implements WitDefinedTool
{
    /**
     * @var array<string, WitToolSchema>
     */
    private static array $witSchemaByPath = [];

    final public static function name(): string
    {
        return static::witSchema()->toolName;
    }

    final public static function description(): string
    {
        return static::witSchema()->toolDescription;
    }

    final public static function endpointName(): string
    {
        return static::name();
    }

    private static function witSchema(): WitToolSchema
    {
        $witPath = static::witPath();

        if (isset(self::$witSchemaByPath[ $witPath ])) {
            return self::$witSchemaByPath[ $witPath ];
        }

        $parser = new WitToolSchemaParser();
        $schema = $parser->parseFile($witPath);
        self::$witSchemaByPath[ $witPath ] = $schema;

        return $schema;
    }

    /**
     * @return class-string
     */
    final public static function generatedTypeClassName(string $suffix): string
    {
        $toolTypeName = preg_replace_callback(
            '/(^|[^a-zA-Z0-9]+)([a-zA-Z0-9])/',
            static fn (array $matches): string => strtoupper($matches[2]),
            static::name(),
        );

        if (!is_string($toolTypeName) || $toolTypeName === '') {
            throw new RuntimeException(sprintf('failed to derive generated type prefix from `%s`', static::name()));
        }

        return static::classNamespace() . '\\' . $toolTypeName . $suffix;
    }

    private static function classNamespace(): string
    {
        $lastSeparatorPosition = strrpos(static::class, '\\');

        if ($lastSeparatorPosition === false) {
            return '';
        }

        return substr(static::class, 0, $lastSeparatorPosition);
    }
}
