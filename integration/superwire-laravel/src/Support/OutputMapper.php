<?php

namespace Superwire\Laravel\Support;

use Spatie\LaravelData\Data;
use Superwire\Laravel\Exceptions\WorkflowExecutionException;

final class OutputMapper
{
    /**
     * @param array<string, mixed> $payload
     * @param class-string<Data> $outputClassName
     */
    public function mapToClass(array $payload, string $outputClassName): object
    {
        if (!is_subclass_of($outputClassName, Data::class)) {
            throw new WorkflowExecutionException(sprintf(
                'failed to map workflow output into %s: class must extend %s',
                $outputClassName,
                Data::class,
            ));
        }

        return $outputClassName::from($payload);
    }
}
