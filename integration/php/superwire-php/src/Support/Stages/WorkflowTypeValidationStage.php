<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Stages;

use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;

final class WorkflowTypeValidationStage
{
    /**
     * @param array<string, mixed> $workflowType
     */
    public function validate(mixed $value, array $workflowType, string $context): void
    {
        $kind = $workflowType[ 'kind' ] ?? null;

        if (!is_string($kind) || $kind === '') {
            throw new InvalidWorkflowDefinitionException("{$context} workflow type is missing `kind`");
        }

        match ($kind) {
            'string' => is_string($value) || $this->throwTypeMismatch($context, 'string', $value),
            'integer' => is_int($value) || $this->throwTypeMismatch($context, 'integer', $value),
            'float' => is_float($value) || is_int($value) || $this->throwTypeMismatch($context, 'float', $value),
            'boolean' => is_bool($value) || $this->throwTypeMismatch($context, 'boolean', $value),
            'null' => $value === null || $this->throwTypeMismatch($context, 'null', $value),
            'array' => $this->validateArray($value, $workflowType, $context),
            'tuple' => $this->validateTuple($value, $workflowType, $context),
            'object' => $this->validateObject($value, $workflowType, $context),
            'union' => $this->validateUnion($value, $workflowType, $context),
            'string_enum' => is_string($value) || $this->throwTypeMismatch($context, 'string_enum', $value),
            default => throw new InvalidWorkflowDefinitionException("{$context} contains unsupported workflow type `{$kind}`"),
        };
    }

    /**
     * @param array<string, mixed> $workflowType
     */
    private function validateArray(mixed $value, array $workflowType, string $context): void
    {
        if (!is_array($value)) {
            $this->throwTypeMismatch($context, 'array', $value);
        }

        $itemType = $workflowType[ 'item_type' ] ?? null;

        if (!is_array($itemType)) {
            throw new InvalidWorkflowDefinitionException("{$context} array type is missing object `item_type`");
        }

        foreach (array_values($value) as $index => $itemValue) {
            $this->validate($itemValue, $itemType, "{$context}[{$index}]");
        }
    }

    /**
     * @param array<string, mixed> $workflowType
     */
    private function validateTuple(mixed $value, array $workflowType, string $context): void
    {
        if (!is_array($value)) {
            $this->throwTypeMismatch($context, 'tuple', $value);
        }

        $tupleItems = $workflowType[ 'items' ] ?? null;

        if (!is_array($tupleItems)) {
            throw new InvalidWorkflowDefinitionException("{$context} tuple type is missing array `items`");
        }

        $normalizedValues = array_values($value);

        if (count($normalizedValues) !== count($tupleItems)) {
            throw new InvalidWorkflowDefinitionException("{$context} tuple value length does not match expected tuple type length");
        }

        foreach (array_values($tupleItems) as $index => $itemType) {

            if (!is_array($itemType)) {
                throw new InvalidWorkflowDefinitionException("{$context} tuple item type at index {$index} must be an object");
            }

            $this->validate($normalizedValues[ $index ], $itemType, "{$context}[{$index}]");

        }
    }

    /**
     * @param array<string, mixed> $workflowType
     */
    private function validateObject(mixed $value, array $workflowType, string $context): void
    {
        if (!is_array($value)) {
            $this->throwTypeMismatch($context, 'object', $value);
        }

        $fields = $workflowType[ 'fields' ] ?? null;

        if (!is_array($fields)) {
            throw new InvalidWorkflowDefinitionException("{$context} object type is missing object `fields`");
        }

        foreach ($fields as $fieldName => $fieldType) {

            if (!is_string($fieldName) || !is_array($fieldType)) {
                throw new InvalidWorkflowDefinitionException("{$context} object type field definitions are invalid");
            }

            if (!array_key_exists($fieldName, $value)) {
                throw new InvalidWorkflowDefinitionException("{$context} output is missing required field `{$fieldName}`");
            }

            $this->validate($value[ $fieldName ], $fieldType, "{$context}.{$fieldName}");

        }
    }

    /**
     * @param array<string, mixed> $workflowType
     */
    private function validateUnion(mixed $value, array $workflowType, string $context): void
    {
        $members = $workflowType[ 'members' ] ?? null;

        if (!is_array($members) || $members === []) {
            throw new InvalidWorkflowDefinitionException("{$context} union type is missing array `members`");
        }

        foreach (array_values($members) as $memberType) {

            if (!is_array($memberType)) {
                continue;
            }

            try {

                $this->validate($value, $memberType, $context);

                return;

            } catch (InvalidWorkflowDefinitionException) {
            }

        }

        throw new InvalidWorkflowDefinitionException("{$context} does not match any allowed union member type");
    }

    private function throwTypeMismatch(string $context, string $expectedType, mixed $value): never
    {
        throw new InvalidWorkflowDefinitionException("{$context} expected `{$expectedType}` but received `" . get_debug_type($value) . '`');
    }
}
