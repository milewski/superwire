<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Wit;

use ReflectionClass;
use RuntimeException;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Tools\Attributes\Description;
use Superwire\Laravel\Wit\Schema\WitSchemaField;
use Superwire\Laravel\Wit\Schema\WitSchemaRecord;
use Superwire\Laravel\Wit\Schema\WitSchemaRecordKind;
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
        $toolTypeName = $this->pascalCase($witToolSchema->toolName);

        $this->writeRecordClass(
            $toolClassDirectory,
            $toolClassNamespace,
            $toolTypeName,
            $witToolSchema->record(WitSchemaRecordKind::AgentInput),
            true,
            false,
        );
        $this->writeRecordClass(
            $toolClassDirectory,
            $toolClassNamespace,
            $toolTypeName,
            $witToolSchema->record(WitSchemaRecordKind::BoundInput),
            false,
            true,
        );
        $this->writeRecordClass(
            $toolClassDirectory,
            $toolClassNamespace,
            $toolTypeName,
            $witToolSchema->record(WitSchemaRecordKind::Output),
            false,
            false,
        );
    }

    private function writeRecordClass(
        string $toolClassDirectory,
        string $toolClassNamespace,
        string $toolTypeName,
        WitSchemaRecord $record,
        bool $isAgentInput,
        bool $isBoundInput,
    ): void {
        $className = $toolTypeName . $record->kind->suffix();
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
            $constructorParameterLines[] = $this->constructorParameterLine($field);
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

        $written = file_put_contents($targetPath, $renderedClass);

        if ($written === false) {
            throw new RuntimeException(sprintf('failed to write generated type class %s', $targetPath));
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

    private function constructorParameterLine(WitSchemaField $field): string
    {
        $descriptionAttribute = is_string($field->description)
            ? sprintf("        #[Description('%s')]\n", $this->escapedPhpString($field->description))
            : '';
        $phpType = $this->phpTypeForWitType($field->witType);
        $nullablePrefix = $field->nullable ? '?' : '';
        $defaultSegment = $field->nullable ? ' = null' : '';

        return $descriptionAttribute . sprintf(
            '        public %s%s $%s%s',
            $nullablePrefix,
            $phpType,
            $field->name,
            $defaultSegment,
        );
    }

    private function phpTypeForWitType(string $witType): string
    {
        return match ($witType) {
            'string' => 'string',
            'bool' => 'bool',
            's32', 'u32', 's64', 'u64' => 'int',
            'f32', 'f64' => 'float',
            default => throw new RuntimeException(sprintf('unsupported WIT type `%s` for PHP generation', $witType)),
        };
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
