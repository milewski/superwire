<?php

namespace App\Superwire;

use RuntimeException;

abstract class Tool
{
    abstract public static function witPath(): string;

    abstract public function execute(object $agentInput, object $boundInput): object;

    public static function name(): string
    {
        return static::directiveValue('tool-name');
    }

    public static function description(): string
    {
        return static::directiveValue('tool-description');
    }

    public static function inputSchema(): array
    {
        return static::recordSchema('agent-input');
    }

    public static function boundInputSchema(): array
    {
        return static::recordSchema('bound-input');
    }

    public static function outputSchema(): array
    {
        return static::recordSchema('output');
    }

    public static function resolveAgentInput(array $agentInputPayload): object
    {
        $agentInputClass = static::agentInputClass();

        return $agentInputClass::fromPayload($agentInputPayload);
    }

    public static function resolveBoundInput(array $boundInputPayload): object
    {
        $boundInputClass = static::boundInputClass();

        return $boundInputClass::fromPayload($boundInputPayload);
    }

    public static function outputPayload(object $output): array
    {
        if (!method_exists($output, 'toPayload')) {
            throw new RuntimeException('tool output must implement toPayload()');
        }

        $payload = $output->toPayload();

        if (!is_array($payload)) {
            throw new RuntimeException('tool output payload must be an array');
        }

        return $payload;
    }

    protected static function agentInputClass(): string
    {
        return static::toolNamespacePrefix() . static::toolTypeName() . 'AgentInput';
    }

    protected static function boundInputClass(): string
    {
        return static::toolNamespacePrefix() . static::toolTypeName() . 'BoundInput';
    }

    protected static function outputClass(): string
    {
        return static::toolNamespacePrefix() . static::toolTypeName() . 'Output';
    }

    private static function toolNamespacePrefix(): string
    {
        return __NAMESPACE__ . '\\Tools\\Generated\\';
    }

    private static function toolTypeName(): string
    {
        $nameSegments = preg_split('/[^a-zA-Z0-9]+/', static::name()) ?: [];
        $toolTypeName = '';

        foreach ($nameSegments as $nameSegment) {
            if ($nameSegment === '') {
                continue;
            }

            $toolTypeName .= ucfirst($nameSegment);
        }

        if ($toolTypeName === '') {
            throw new RuntimeException('failed to derive tool type name from WIT metadata');
        }

        return $toolTypeName;
    }

    private static function directiveValue(string $directiveName): string
    {
        $witSource = static::witSource();
        $directivePrefix = sprintf('/// @%s ', $directiveName);

        foreach (preg_split('/\R/', $witSource) ?: [] as $sourceLine) {
            $trimmedLine = trim($sourceLine);

            if (str_starts_with($trimmedLine, $directivePrefix)) {
                return trim(substr($trimmedLine, strlen($directivePrefix)));
            }
        }

        throw new RuntimeException(sprintf('missing `%s` directive in %s', $directiveName, static::witPath()));
    }

    private static function recordSchema(string $recordName): array
    {
        $sourceLines = preg_split('/\R/', static::witSource()) ?: [];
        $insideRecord = false;
        $recordDescription = null;
        $pendingDocs = [];
        $properties = [];
        $requiredProperties = [];

        foreach ($sourceLines as $sourceLine) {
            $trimmedLine = trim($sourceLine);

            if (str_starts_with($trimmedLine, '///')) {
                $pendingDocs[] = trim(substr($trimmedLine, 3));

                continue;
            }

            if (!$insideRecord && $trimmedLine === sprintf('record %s {', $recordName)) {
                $insideRecord = true;
                $recordDescription = $pendingDocs === [] ? null : trim(implode(' ', $pendingDocs));
                $pendingDocs = [];

                continue;
            }

            if ($insideRecord && $trimmedLine === '}') {
                break;
            }

            if (!$insideRecord) {
                $pendingDocs = [];

                continue;
            }

            if (preg_match('/^([a-z][a-z0-9\-]*)\s*:\s*([^,]+),$/', $trimmedLine, $matches) !== 1) {
                $pendingDocs = [];

                continue;
            }

            $fieldName = str_replace('-', '_', $matches[1]);
            $fieldType = trim($matches[2]);
            $nullable = false;

            if (preg_match('/^option<(.+)>$/', $fieldType, $optionMatches) === 1) {
                $nullable = true;
                $fieldType = trim($optionMatches[1]);
            }

            $jsonSchemaType = match ($fieldType) {
                'string' => 'string',
                'bool' => 'boolean',
                's32', 'u32', 's64', 'u64' => 'integer',
                'f32', 'f64' => 'number',
                default => throw new RuntimeException(sprintf('unsupported WIT type `%s` in %s', $fieldType, static::witPath())),
            };

            $propertySchema = [
                'type' => $nullable ? [ $jsonSchemaType, 'null' ] : $jsonSchemaType,
            ];

            if ($pendingDocs !== []) {
                $propertySchema['description'] = trim(implode(' ', $pendingDocs));
            }

            $properties[ $fieldName ] = $propertySchema;

            if (!$nullable) {
                $requiredProperties[] = $fieldName;
            }

            $pendingDocs = [];
        }

        $schema = [
            'type' => 'object',
            'properties' => $properties,
        ];

        if (is_string($recordDescription) && $recordDescription !== '') {
            $schema['description'] = $recordDescription;
        }

        if ($requiredProperties !== []) {
            $schema['required'] = $requiredProperties;
        }

        return $schema;
    }

    private static function witSource(): string
    {
        $witSource = file_get_contents(static::witPath());

        if (!is_string($witSource) || $witSource === '') {
            throw new RuntimeException(sprintf('failed to read WIT source at %s', static::witPath()));
        }

        return $witSource;
    }
}
