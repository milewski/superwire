<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Stages;

final class WorkflowTypeNormalizationStage
{
    /**
     * @param array<string, mixed> $workflowType
     */
    public function normalize(mixed $value, array $workflowType): mixed
    {
        $kind = $workflowType[ 'kind' ] ?? null;

        if (!is_string($kind)) {
            return $value;
        }

        if ($kind === 'object') {

            $fields = $workflowType[ 'fields' ] ?? null;

            if (!is_array($fields)) {
                return is_array($value) ? $value : [];
            }

            $normalizedObject = is_array($value) ? $value : [];

            foreach ($fields as $fieldName => $fieldType) {

                if (!is_string($fieldName) || !is_array($fieldType)) {
                    continue;
                }

                $normalizedObject[ $fieldName ] = $this->normalize($normalizedObject[ $fieldName ] ?? null, $fieldType);

            }

            return $normalizedObject;

        }

        if ($kind === 'array') {

            $itemType = $workflowType[ 'item_type' ] ?? null;

            if (!is_array($itemType)) {
                return is_array($value) ? array_values($value) : [];
            }

            if (!is_array($value)) {
                return [];
            }

            return array_map(
                fn (mixed $item): mixed => $this->normalize($item, $itemType),
                array_values($value),
            );

        }

        if ($kind === 'tuple') {

            $items = $workflowType[ 'items' ] ?? null;

            if (!is_array($items)) {
                return is_array($value) ? array_values($value) : [];
            }

            $tupleValues = is_array($value) ? array_values($value) : [];
            $normalizedTuple = [];

            foreach (array_values($items) as $index => $itemType) {

                if (!is_array($itemType)) {
                    continue;
                }

                $normalizedTuple[] = $this->normalize($tupleValues[ $index ] ?? null, $itemType);

            }

            return $normalizedTuple;

        }

        if ($kind === 'union') {

            $members = $workflowType[ 'members' ] ?? null;

            if (!is_array($members) || $members === []) {
                return $value;
            }

            foreach ($members as $memberType) {

                if (!is_array($memberType)) {
                    continue;
                }

                return $this->normalize($value, $memberType);

            }

            return $value;

        }

        return $this->normalizeScalar($value, $kind);
    }

    private function normalizeScalar(mixed $value, string $kind): mixed
    {
        if ($kind === 'string') {
            return is_string($value) ? $value : '';
        }

        if ($kind === 'integer') {

            if (is_int($value)) {
                return $value;
            }

            if (is_numeric($value)) {
                return (int) $value;
            }

            return 0;

        }

        if ($kind === 'float') {

            if (is_float($value) || is_int($value)) {
                return (float) $value;
            }

            if (is_numeric($value)) {
                return (float) $value;
            }

            return 0.0;

        }

        if ($kind === 'boolean') {
            return is_bool($value) ? $value : false;
        }

        if ($kind === 'null') {
            return null;
        }

        if ($kind === 'string_enum') {
            return is_string($value) ? $value : '';
        }

        return $value;
    }
}
