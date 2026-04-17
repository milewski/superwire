<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit;

use RuntimeException;
use Superwire\Laravel\Wit\Schema\WitSchemaField;
use Superwire\Laravel\Wit\Schema\WitSchemaRecord;
use Superwire\Laravel\Wit\Schema\WitSchemaRecordKind;
use Superwire\Laravel\Wit\Schema\WitToolSchema;

final class WitToolSchemaParser
{
    public function parseFile(string $witFilePath): WitToolSchema
    {
        if (!is_file($witFilePath)) {
            throw new RuntimeException(sprintf('WIT file does not exist: %s', $witFilePath));
        }

        $witSource = file_get_contents($witFilePath);

        if (!is_string($witSource) || $witSource === '') {
            throw new RuntimeException(sprintf('failed to read WIT file: %s', $witFilePath));
        }

        return $this->parseSource($witSource, $witFilePath);
    }

    public function parseSource(string $witSource, string $sourceLabel = '<memory>'): WitToolSchema
    {
        $toolName = $this->directiveValue($witSource, 'tool-name', $sourceLabel);
        $toolDescription = $this->directiveValue($witSource, 'tool-description', $sourceLabel);
        $sourceLines = preg_split('/\R/', $witSource) ?: [];
        $recordsByKind = [];
        $insideSchemaInterface = false;
        $currentRecordKind = null;
        $recordDescription = null;
        $recordFields = [];
        $pendingDocs = [];

        foreach ($sourceLines as $sourceLine) {
            $trimmedLine = trim($sourceLine);

            if (str_starts_with($trimmedLine, '///')) {
                $pendingDocs[] = trim(substr($trimmedLine, 3));

                continue;
            }

            if ($trimmedLine === 'interface schema {') {
                $insideSchemaInterface = true;

                continue;
            }

            if ($insideSchemaInterface && $trimmedLine === '}' && $currentRecordKind === null) {
                $insideSchemaInterface = false;

                continue;
            }

            if (!$insideSchemaInterface) {
                $pendingDocs = [];

                continue;
            }

            if ($currentRecordKind === null) {
                $resolvedRecordKind = $this->recordKindFromLine($trimmedLine);

                if ($resolvedRecordKind === null) {
                    $pendingDocs = [];

                    continue;
                }

                $currentRecordKind = $resolvedRecordKind;
                $recordDescription = $this->joinedDocs($pendingDocs);
                $recordFields = [];
                $pendingDocs = [];

                continue;
            }

            if ($trimmedLine === '}') {
                $recordsByKind[ $currentRecordKind->value ] = new WitSchemaRecord(
                    $currentRecordKind,
                    $recordDescription,
                    $recordFields,
                );
                $currentRecordKind = null;
                $recordDescription = null;
                $recordFields = [];
                $pendingDocs = [];

                continue;
            }

            $field = $this->fieldFromLine($trimmedLine, $pendingDocs, $sourceLabel);

            if ($field instanceof WitSchemaField) {
                $recordFields[] = $field;
            }

            $pendingDocs = [];
        }

        return new WitToolSchema(
            $toolName,
            $toolDescription,
            $recordsByKind,
        );
    }

    private function directiveValue(string $witSource, string $directiveName, string $sourceLabel): string
    {
        $directivePrefix = sprintf('/// @%s ', $directiveName);
        $sourceLines = preg_split('/\R/', $witSource) ?: [];

        foreach ($sourceLines as $sourceLine) {
            $trimmedLine = trim($sourceLine);

            if (str_starts_with($trimmedLine, $directivePrefix)) {
                return trim(substr($trimmedLine, strlen($directivePrefix)));
            }
        }

        throw new RuntimeException(sprintf('missing `%s` directive in %s', $directiveName, $sourceLabel));
    }

    private function recordKindFromLine(string $trimmedLine): ?WitSchemaRecordKind
    {
        if (preg_match('/^record\s+(agent-input|bound-input|output)\s*\{$/', $trimmedLine, $matches) !== 1) {
            return null;
        }

        return WitSchemaRecordKind::from($matches[1]);
    }

    /**
     * @param list<string> $pendingDocs
     */
    private function fieldFromLine(string $trimmedLine, array $pendingDocs, string $sourceLabel): ?WitSchemaField
    {
        if (preg_match('/^([a-z][a-z0-9\-]*)\s*:\s*([^,]+),$/', $trimmedLine, $matches) !== 1) {
            return null;
        }

        $fieldName = str_replace('-', '_', $matches[1]);
        $fieldType = trim($matches[2]);
        $nullable = false;

        if (preg_match('/^option<(.+)>$/', $fieldType, $optionMatches) === 1) {
            $nullable = true;
            $fieldType = trim($optionMatches[1]);
        }

        if (!in_array($fieldType, [ 'string', 'bool', 's32', 'u32', 's64', 'u64', 'f32', 'f64' ], true)) {
            throw new RuntimeException(sprintf('unsupported WIT type `%s` in %s', $fieldType, $sourceLabel));
        }

        return new WitSchemaField(
            $fieldName,
            $fieldType,
            $nullable,
            $this->joinedDocs($pendingDocs),
        );
    }

    /**
     * @param list<string> $pendingDocs
     */
    private function joinedDocs(array $pendingDocs): ?string
    {
        if ($pendingDocs === []) {
            return null;
        }

        $joinedDocs = trim(implode(' ', array_map('trim', $pendingDocs)));

        return $joinedDocs === '' ? null : $joinedDocs;
    }
}
