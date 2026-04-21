<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use RuntimeException;
use Throwable;

final readonly class ForkExecutionFailure
{
    public function __construct(
        public string $exceptionClass,
        public string $message,
        public string $file,
        public int $line,
    )
    {
    }

    public static function fromThrowable(Throwable $throwable): self
    {
        return new self(
            exceptionClass: $throwable::class,
            message: $throwable->getMessage(),
            file: $throwable->getFile(),
            line: $throwable->getLine(),
        );
    }

    public function toRuntimeException(string $context): RuntimeException
    {
        return new RuntimeException(sprintf(
            'Execution failed for %s: %s: %s in %s:%d.',
            $context,
            $this->exceptionClass,
            $this->message,
            $this->file,
            $this->line,
        ));
    }
}
