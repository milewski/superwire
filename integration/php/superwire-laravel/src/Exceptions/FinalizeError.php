<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Exceptions;

use RuntimeException;

final class FinalizeError extends RuntimeException
{
    public function __construct(
        public readonly string $reason,
    )
    {
        parent::__construct($reason);
    }
}
