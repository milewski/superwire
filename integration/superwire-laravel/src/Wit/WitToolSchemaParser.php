<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit;

use RuntimeException;
use Superwire\Laravel\Wit\Schema\WitSchemaEnum;
use Superwire\Laravel\Wit\Schema\WitSchemaField;
use Superwire\Laravel\Wit\Schema\WitSchemaRecord;
use Superwire\Laravel\Wit\Schema\WitSchemaRecordKind;
use Superwire\Laravel\Wit\Schema\WitSchemaVariant;
use Superwire\Laravel\Wit\Schema\WitSchemaVariantCase;
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
        $namedRecords = [];
        $namedEnums = [];
        $namedVariants = [];
        $insideSchemaInterface = false;
        $currentDeclarationKind = null;
        $currentDeclarationName = null;
        $currentRecordKind = null;
        $declarationDescription = null;
        $recordFields = [];
        $enumCases = [];
        $variantCases = [];
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

            if ($insideSchemaInterface && $trimmedLine === '}' && $currentDeclarationKind === null) {

                $insideSchemaInterface = false;

                continue;

            }

            if (!$insideSchemaInterface) {

                $pendingDocs = [];

                continue;

            }

            if ($currentDeclarationKind === null) {

                [
                    'declaration_kind' => $resolvedDeclarationKind,
                    'record_kind' => $resolvedRecordKind,
                    'declaration_name' => $resolvedDeclarationName,
                ] = $this->declarationFromLine($trimmedLine);

                if ($resolvedDeclarationKind === null || $resolvedDeclarationName === null) {

                    $pendingDocs = [];

                    continue;

                }

                $currentDeclarationKind = $resolvedDeclarationKind;
                $currentRecordKind = $resolvedRecordKind;
                $currentDeclarationName = $resolvedDeclarationName;
                $declarationDescription = $this->joinedDocs($pendingDocs);
                $recordFields = [];
                $enumCases = [];
                $variantCases = [];
                $pendingDocs = [];

                continue;

            }

            if ($trimmedLine === '}') {

                if (!is_string($currentDeclarationName)) {
                    throw new RuntimeException(sprintf('failed to resolve record name in %s', $sourceLabel));
                }

                if ($currentDeclarationKind === 'record') {

                    $record = new WitSchemaRecord(
                        $currentDeclarationName,
                        $currentRecordKind,
                        $declarationDescription,
                        $recordFields,
                    );

                    if ($currentRecordKind instanceof WitSchemaRecordKind) {
                        $recordsByKind[ $currentRecordKind->value ] = $record;
                    }

                    $namedRecords[ $currentDeclarationName ] = $record;

                } elseif ($currentDeclarationKind === 'enum') {

                    $namedEnums[ $currentDeclarationName ] = new WitSchemaEnum(
                        $currentDeclarationName,
                        $declarationDescription,
                        $enumCases,
                    );

                } elseif ($currentDeclarationKind === 'variant') {

                    $namedVariants[ $currentDeclarationName ] = new WitSchemaVariant(
                        $currentDeclarationName,
                        $declarationDescription,
                        $variantCases,
                    );

                }

                $currentDeclarationKind = null;
                $currentRecordKind = null;
                $currentDeclarationName = null;
                $declarationDescription = null;
                $recordFields = [];
                $enumCases = [];
                $variantCases = [];
                $pendingDocs = [];

                continue;

            }

            if ($currentDeclarationKind === 'enum') {

                $enumCaseName = $this->enumCaseNameFromLine($trimmedLine);

                if ($enumCaseName !== null) {
                    $enumCases[] = $enumCaseName;
                }

                $pendingDocs = [];

                continue;

            }

            if ($currentDeclarationKind === 'variant') {

                $variantCase = $this->variantCaseFromLine($trimmedLine, $pendingDocs);

                if ($variantCase instanceof WitSchemaVariantCase) {
                    $variantCases[] = $variantCase;
                }

                $pendingDocs = [];

                continue;

            }

            $field = $this->fieldFromLine($trimmedLine, $pendingDocs, $sourceLabel);

            if ($field instanceof WitSchemaField) {
                $recordFields[] = $field;
            }

            $pendingDocs = [];

        }

        $this->assertTypesAreSupported($namedRecords, $namedEnums, $namedVariants, $sourceLabel);

        return new WitToolSchema(
            $toolName,
            $toolDescription,
            $recordsByKind,
            $namedRecords,
            $namedEnums,
            $namedVariants,
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

    /**
     * @return array{declaration_kind: string|null, record_kind: WitSchemaRecordKind|null, declaration_name: string|null}
     */
    private function declarationFromLine(string $trimmedLine): array
    {
        if (preg_match('/^(record|enum|variant)\s+([a-z][a-z0-9_\-]*)\s*\{$/', $trimmedLine, $matches) !== 1) {
            return [ 'declaration_kind' => null, 'record_kind' => null, 'declaration_name' => null ];
        }

        if ($matches[ 1 ] !== 'record') {

            return [
                'declaration_kind' => $matches[ 1 ],
                'record_kind' => null,
                'declaration_name' => str_replace('-', '_', $matches[ 2 ]),
            ];

        }

        return [
            'declaration_kind' => 'record',
            'record_kind' => WitSchemaRecordKind::tryFrom($matches[ 2 ]),
            'declaration_name' => str_replace('-', '_', $matches[ 2 ]),
        ];
    }

    /**
     * @param list<string> $pendingDocs
     */
    private function fieldFromLine(string $trimmedLine, array $pendingDocs, string $sourceLabel): ?WitSchemaField
    {
        if (preg_match('/^([a-z][a-z0-9_\-]*)\s*:\s*([^,]+),$/', $trimmedLine, $matches) !== 1) {
            return null;
        }

        $fieldName = str_replace('-', '_', $matches[ 1 ]);
        $fieldType = str_replace('-', '_', trim($matches[ 2 ]));
        $nullable = false;

        if (preg_match('/^option<(.+)>$/', $fieldType, $optionMatches) === 1) {

            $nullable = true;
            $fieldType = trim($optionMatches[ 1 ]);

        }

        return new WitSchemaField(
            $fieldName,
            $fieldType,
            $nullable,
            $this->joinedDocs($pendingDocs),
        );
    }

    /**
     * @param array<string, WitSchemaRecord> $recordsByName
     * @param array<string, WitSchemaEnum> $enumsByName
     * @param array<string, WitSchemaVariant> $variantsByName
     */
    private function assertTypesAreSupported(
        array $recordsByName,
        array $enumsByName,
        array $variantsByName,
        string $sourceLabel,
    ): void
    {
        $knownNamedTypes = array_values(array_unique(array_merge(
            array_keys($recordsByName),
            array_keys($enumsByName),
            array_keys($variantsByName),
        )));

        foreach ($recordsByName as $record) {

            foreach ($record->fields as $field) {
                $this->assertFieldTypeIsSupported($field->witType, $knownNamedTypes, $sourceLabel);
            }

        }

        foreach ($variantsByName as $variant) {

            foreach ($variant->cases as $variantCase) {

                if ($variantCase->payloadType === null) {
                    continue;
                }

                $this->assertFieldTypeIsSupported($variantCase->payloadType, $knownNamedTypes, $sourceLabel);

            }

        }
    }

    /**
     * @param list<string> $knownRecordTypes
     */
    private function assertFieldTypeIsSupported(string $fieldType, array $knownRecordTypes, string $sourceLabel): void
    {
        $primitiveTypes = [ 'string', 'bool', 's32', 'u32', 's64', 'u64', 'f32', 'f64' ];

        if (in_array($fieldType, $primitiveTypes, true)) {
            return;
        }

        if (preg_match('/^list<(.+)>$/', $fieldType, $listMatches) === 1) {

            $this->assertFieldTypeIsSupported(trim($listMatches[ 1 ]), $knownRecordTypes, $sourceLabel);

            return;

        }

        if (in_array($fieldType, $knownRecordTypes, true)) {
            return;
        }

        throw new RuntimeException(sprintf('unsupported WIT type `%s` in %s', $fieldType, $sourceLabel));
    }

    private function enumCaseNameFromLine(string $trimmedLine): ?string
    {
        if (preg_match('/^([a-z][a-z0-9_\-]*),$/', $trimmedLine, $matches) !== 1) {
            return null;
        }

        return str_replace('-', '_', $matches[ 1 ]);
    }

    /**
     * @param list<string> $pendingDocs
     */
    private function variantCaseFromLine(string $trimmedLine, array $pendingDocs): ?WitSchemaVariantCase
    {
        if (preg_match('/^([a-z][a-z0-9_\-]*)\((.+)\),$/', $trimmedLine, $matches) === 1) {

            return new WitSchemaVariantCase(
                name: str_replace('-', '_', $matches[ 1 ]),
                payloadType: str_replace('-', '_', trim($matches[ 2 ])),
                description: $this->joinedDocs($pendingDocs),
            );

        }

        if (preg_match('/^([a-z][a-z0-9_\-]*),$/', $trimmedLine, $matches) === 1) {

            return new WitSchemaVariantCase(
                name: str_replace('-', '_', $matches[ 1 ]),
                payloadType: null,
                description: $this->joinedDocs($pendingDocs),
            );

        }

        return null;
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
