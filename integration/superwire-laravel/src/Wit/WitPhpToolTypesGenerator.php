<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit;

use ReflectionClass;
use RuntimeException;
use Spatie\LaravelData\Attributes\DataCollectionOf;
use Spatie\LaravelData\DataCollection;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Tools\Attributes\Description;
use Superwire\Laravel\Wit\Schema\WitSchemaEnum;
use Superwire\Laravel\Wit\Schema\WitSchemaField;
use Superwire\Laravel\Wit\Schema\WitSchemaRecord;
use Superwire\Laravel\Wit\Schema\WitSchemaRecordKind;
use Superwire\Laravel\Wit\Schema\WitSchemaVariant;
use Superwire\Laravel\Wit\Schema\WitSchemaVariantCase;
use Superwire\Laravel\Wit\Schema\WitToolSchema;

final class WitPhpToolTypesGenerator
{
    /**
     * @param class-string $toolClass
     */
    public function generateForToolClass(string $toolClass, WitToolSchema $witToolSchema): void
    {
        $toolClassReflection = new ReflectionClass($toolClass);
        $toolClassDirectory = dirname((string) $toolClassReflection->getFileName());
        $toolClassNamespace = $toolClassReflection->getNamespaceName();
        $toolDataDirectory = $toolClassDirectory . DIRECTORY_SEPARATOR . 'Data';
        $toolDataNamespace = $toolClassNamespace . '\\Data';
        $toolTypeName = $this->pascalCase($witToolSchema->toolName);

        if (!is_dir($toolDataDirectory) && !mkdir($toolDataDirectory, 0o777, true) && !is_dir($toolDataDirectory)) {
            throw new RuntimeException(sprintf('failed to create generated type directory %s', $toolDataDirectory));
        }

        $namedRecordClassNames = [];
        $namedEnumClassNames = [];
        $namedVariantClassNames = [];

        foreach ($witToolSchema->namedEnums as $enumName => $namedEnum) {

            $namedEnumClassNames[ $enumName ] = $this->pascalCase($enumName);
            $this->writeEnumClass($toolDataDirectory, $toolDataNamespace, $namedEnumClassNames[ $enumName ], $namedEnum);

        }

        foreach ($witToolSchema->namedRecords as $recordName => $namedRecord) {

            if ($namedRecord->kind instanceof WitSchemaRecordKind) {
                continue;
            }

            $namedRecordClassNames[ $recordName ] = $this->pascalCase($recordName);

        }

        foreach ($witToolSchema->namedVariants as $variantName => $_variant) {
            $namedVariantClassNames[ $variantName ] = $this->pascalCase($variantName);
        }

        foreach ($witToolSchema->namedVariants as $variantName => $namedVariant) {

            $this->writeVariantClass(
                $toolDataDirectory,
                $toolDataNamespace,
                $namedVariantClassNames[ $variantName ],
                $namedVariant,
                $namedRecordClassNames,
                $namedEnumClassNames,
                $namedVariantClassNames,
            );

        }

        foreach ($witToolSchema->namedRecords as $recordName => $namedRecord) {

            if ($namedRecord->kind instanceof WitSchemaRecordKind) {
                continue;
            }

            $this->writeRecordClass(
                $toolDataDirectory,
                $toolDataNamespace,
                $namedRecordClassNames[ $recordName ],
                $namedRecord,
                $namedRecordClassNames,
                $namedEnumClassNames,
                $namedVariantClassNames,
                false,
                false,
            );

        }

        if ($witToolSchema->hasRecord(WitSchemaRecordKind::AgentInput)) {

            $this->writeRecordClass(
                $toolDataDirectory,
                $toolDataNamespace,
                $toolTypeName . WitSchemaRecordKind::AgentInput->suffix(),
                $witToolSchema->record(WitSchemaRecordKind::AgentInput),
                $namedRecordClassNames,
                $namedEnumClassNames,
                $namedVariantClassNames,
                true,
                false,
            );

        }

        if ($witToolSchema->hasRecord(WitSchemaRecordKind::BoundInput)) {

            $this->writeRecordClass(
                $toolDataDirectory,
                $toolDataNamespace,
                $toolTypeName . WitSchemaRecordKind::BoundInput->suffix(),
                $witToolSchema->record(WitSchemaRecordKind::BoundInput),
                $namedRecordClassNames,
                $namedEnumClassNames,
                $namedVariantClassNames,
                false,
                true,
            );

        }

        if ($witToolSchema->hasRecord(WitSchemaRecordKind::Output)) {

            $this->writeRecordClass(
                $toolDataDirectory,
                $toolDataNamespace,
                $toolTypeName . WitSchemaRecordKind::Output->suffix(),
                $witToolSchema->record(WitSchemaRecordKind::Output),
                $namedRecordClassNames,
                $namedEnumClassNames,
                $namedVariantClassNames,
                false,
                false,
            );

        }
    }

    /**
     * @param array<string, string> $namedRecordClassNames
     * @param array<string, string> $namedEnumClassNames
     * @param array<string, string> $namedVariantClassNames
     */
    private function writeRecordClass(
        string $toolClassDirectory,
        string $toolClassNamespace,
        string $className,
        WitSchemaRecord $record,
        array $namedRecordClassNames,
        array $namedEnumClassNames,
        array $namedVariantClassNames,
        bool $isAgentInput,
        bool $isBoundInput,
    ): void {
        $targetPath = $toolClassDirectory . DIRECTORY_SEPARATOR . $className . '.php';
        $interfaceImport = null;
        $interfaceName = null;

        if ($isAgentInput) {

            $interfaceImport = ToolInputData::class;
            $interfaceName = 'ToolInputData';

        }

        if ($isBoundInput) {

            $interfaceImport = ToolBoundInputData::class;
            $interfaceName = 'ToolBoundInputData';

        }

        $useStatements = [
            'Spatie\\LaravelData\\Data',
            Description::class,
        ];

        if (is_string($interfaceImport)) {
            $useStatements[] = $interfaceImport;
        }

        $classDescriptionAttribute = is_string($record->description)
            ? sprintf("#[Description('%s')]\n", $this->escapedPhpString($record->description))
            : '';

        $constructorParameterLines = [];

        foreach ($record->fields as $field) {

            $constructorParameterLines[] = $this->constructorParameterLine(
                $field,
                $namedRecordClassNames,
                $namedEnumClassNames,
                $namedVariantClassNames,
            );

            if ($this->isListOfNamedRecord($field->witType, $namedRecordClassNames)) {

                $useStatements[] = DataCollection::class;
                $useStatements[] = DataCollectionOf::class;

            }

        }

        $implementedInterfaceSegment = is_string($interfaceName) ? sprintf(' implements %s', $interfaceName) : '';

        $renderedClass = "<?php\n\ndeclare(strict_types = 1);\n\nnamespace {$toolClassNamespace};\n\n"
            . $this->renderUseStatements($useStatements)
            . "\n"
            . $classDescriptionAttribute
            . "final class {$className} extends Data{$implementedInterfaceSegment}\n"
            . "{\n"
            . "    public function __construct(\n"
            . implode(",\n", $constructorParameterLines)
            . "\n    )\n"
            . "    {\n"
            . "    }\n"
            . "}\n";

        if (file_put_contents($targetPath, $renderedClass) === false) {
            throw new RuntimeException(sprintf('failed to write generated type class %s', $targetPath));
        }
    }

    private function writeEnumClass(
        string $toolClassDirectory,
        string $toolClassNamespace,
        string $className,
        WitSchemaEnum $enum,
    ): void {
        $targetPath = $toolClassDirectory . DIRECTORY_SEPARATOR . $className . '.php';

        $caseLines = array_map(
            static fn (string $caseName): string => sprintf(
                "    case %s = '%s';",
                strtoupper($caseName),
                $caseName,
            ),
            $enum->cases,
        );

        $renderedClass = "<?php\n\ndeclare(strict_types = 1);\n\nnamespace {$toolClassNamespace};\n\n"
            . "enum {$className}: string\n"
            . "{\n"
            . implode("\n", $caseLines)
            . "\n}\n";

        if (file_put_contents($targetPath, $renderedClass) === false) {
            throw new RuntimeException(sprintf('failed to write generated enum class %s', $targetPath));
        }
    }

    /**
     * @param array<string, string> $namedRecordClassNames
     * @param array<string, string> $namedEnumClassNames
     * @param array<string, string> $namedVariantClassNames
     */
    private function writeVariantClass(
        string $toolClassDirectory,
        string $toolClassNamespace,
        string $className,
        WitSchemaVariant $variant,
        array $namedRecordClassNames,
        array $namedEnumClassNames,
        array $namedVariantClassNames,
    ): void {
        $kindEnumClassName = $className . 'Kind';
        $kindEnum = new WitSchemaEnum(
            name: $variant->name . '_kind',
            description: null,
            cases: array_map(static fn (WitSchemaVariantCase $case): string => $case->name, $variant->cases),
        );

        $this->writeEnumClass($toolClassDirectory, $toolClassNamespace, $kindEnumClassName, $kindEnum);

        $factoryLines = [];

        foreach ($variant->cases as $variantCase) {

            $factoryMethodName = $variantCase->name;
            $factoryKindCase = strtoupper($variantCase->name);

            if ($variantCase->payloadType === null) {

                $factoryLines[] = sprintf(
                    "    public static function %s(): self\n    {\n        return new self(kind: %s::%s);\n    }",
                    $factoryMethodName,
                    $kindEnumClassName,
                    $factoryKindCase,
                );

                continue;

            }

            $payloadType = $this->phpTypeForWitType(
                $variantCase->payloadType,
                $namedRecordClassNames,
                $namedEnumClassNames,
                $namedVariantClassNames,
            );

            $factoryLines[] = sprintf(
                "    public static function %s(%s \$value): self\n    {\n        return new self(kind: %s::%s, value: \$value);\n    }",
                $factoryMethodName,
                $payloadType,
                $kindEnumClassName,
                $factoryKindCase,
            );

        }

        $targetPath = $toolClassDirectory . DIRECTORY_SEPARATOR . $className . '.php';

        $renderedClass = "<?php\n\ndeclare(strict_types = 1);\n\nnamespace {$toolClassNamespace};\n\n"
            . "use Spatie\\LaravelData\\Data;\n\n"
            . "final class {$className} extends Data\n"
            . "{\n"
            . "    public function __construct(\n"
            . "        public {$kindEnumClassName} \$kind,\n"
            . "        public mixed \$value = null,\n"
            . "    )\n"
            . "    {\n"
            . "    }\n\n"
            . implode("\n\n", $factoryLines)
            . "\n}\n";

        if (file_put_contents($targetPath, $renderedClass) === false) {
            throw new RuntimeException(sprintf('failed to write generated variant class %s', $targetPath));
        }
    }

    private function renderUseStatements(array $useStatements): string
    {
        $uniqueUseStatements = array_values(array_unique($useStatements));
        sort($uniqueUseStatements);

        return implode("\n", array_map(
            static fn (string $useStatement): string => sprintf('use %s;', $useStatement),
            $uniqueUseStatements,
        )) . "\n";
    }

    /**
     * @param array<string, string> $namedRecordClassNames
     * @param array<string, string> $namedEnumClassNames
     * @param array<string, string> $namedVariantClassNames
     */
    private function constructorParameterLine(
        WitSchemaField $field,
        array $namedRecordClassNames,
        array $namedEnumClassNames,
        array $namedVariantClassNames,
    ): string {
        $attributeLines = [];

        if (is_string($field->description)) {
            $attributeLines[] = sprintf("        #[Description('%s')]", $this->escapedPhpString($field->description));
        }

        if ($this->isListOfNamedRecord($field->witType, $namedRecordClassNames)) {

            $attributeLines[] = sprintf(
                '        #[DataCollectionOf(%s::class)]',
                $this->classNameForType($this->innerListType($field->witType), $namedRecordClassNames),
            );

        }

        $phpType = $this->phpTypeForWitType(
            $field->witType,
            $namedRecordClassNames,
            $namedEnumClassNames,
            $namedVariantClassNames,
        );
        $nullablePrefix = $field->nullable ? '?' : '';
        $defaultSegment = $field->nullable ? ' = null' : '';

        return ($attributeLines !== [] ? implode("\n", $attributeLines) . "\n" : '') . sprintf(
            '        public %s%s $%s%s',
            $nullablePrefix,
            $phpType,
            $field->name,
            $defaultSegment,
        );
    }

    /**
     * @param array<string, string> $namedRecordClassNames
     * @param array<string, string> $namedEnumClassNames
     * @param array<string, string> $namedVariantClassNames
     */
    private function phpTypeForWitType(
        string $witType,
        array $namedRecordClassNames,
        array $namedEnumClassNames,
        array $namedVariantClassNames,
    ): string {
        if ($this->isListType($witType)) {
            return $this->isListOfNamedRecord($witType, $namedRecordClassNames) ? 'DataCollection' : 'array';
        }

        if (isset($namedRecordClassNames[ $witType ])) {
            return $namedRecordClassNames[ $witType ];
        }

        if (isset($namedEnumClassNames[ $witType ])) {
            return $namedEnumClassNames[ $witType ];
        }

        if (isset($namedVariantClassNames[ $witType ])) {
            return $namedVariantClassNames[ $witType ];
        }

        return match ($witType) {
            'string' => 'string',
            'bool' => 'bool',
            's32', 'u32', 's64', 'u64' => 'int',
            'f32', 'f64' => 'float',
            default => throw new RuntimeException(sprintf('unsupported WIT type `%s` for PHP generation', $witType)),
        };
    }

    private function isListType(string $witType): bool
    {
        return preg_match('/^list<(.+)>$/', $witType) === 1;
    }

    private function innerListType(string $witType): string
    {
        preg_match('/^list<(.+)>$/', $witType, $matches);

        return trim((string) ($matches[ 1 ] ?? ''));
    }

    /**
     * @param array<string, string> $namedRecordClassNames
     */
    private function isListOfNamedRecord(string $witType, array $namedRecordClassNames): bool
    {
        if (!$this->isListType($witType)) {
            return false;
        }

        return isset($namedRecordClassNames[ $this->innerListType($witType) ]);
    }

    /**
     * @param array<string, string> $namedRecordClassNames
     */
    private function classNameForType(string $witType, array $namedRecordClassNames): string
    {
        if (isset($namedRecordClassNames[ $witType ])) {
            return $namedRecordClassNames[ $witType ];
        }

        throw new RuntimeException(sprintf('unsupported named type `%s` for PHP generation', $witType));
    }

    private function pascalCase(string $value): string
    {
        $segments = preg_split('/[^a-zA-Z0-9]+/', $value) ?: [];
        $convertedValue = '';

        foreach ($segments as $segment) {

            if ($segment === '') {
                continue;
            }

            $convertedValue .= ucfirst($segment);

        }

        if ($convertedValue === '') {
            throw new RuntimeException(sprintf('failed to convert `%s` into class name prefix', $value));
        }

        return $convertedValue;
    }

    private function escapedPhpString(string $value): string
    {
        return str_replace([ '\\', "'" ], [ '\\\\', "\\'" ], $value);
    }
}
