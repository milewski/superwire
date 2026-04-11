<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Execution\Compiler;

use Superwire\Laravel\Contracts\Tool;
use Superwire\Laravel\Exceptions\InvalidToolClassException;

final class ToolClassValidator
{
    /**
     * @param list<class-string> $toolClasses
     * @return list<class-string<Tool>>
     */
    public function validate(array $toolClasses): array
    {
        $validatedToolClasses = [];

        foreach ($toolClasses as $toolClass) {

            if (!is_string($toolClass)) {
                throw new InvalidToolClassException('tool class references must be class-string values');
            }

            if (!class_exists($toolClass)) {
                throw new InvalidToolClassException(sprintf('tool class `%s` does not exist', $toolClass));
            }

            if (!is_subclass_of($toolClass, Tool::class)) {
                throw new InvalidToolClassException(sprintf('tool class `%s` must implement %s', $toolClass, Tool::class));
            }

            $validatedToolClasses[] = $toolClass;

        }

        return $validatedToolClasses;
    }
}
