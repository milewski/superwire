<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Exceptions;

use RuntimeException;

final class FinalizeSuccess extends RuntimeException
{
    public function __construct(
        public readonly mixed $result,
    )
    {
        parent::__construct('Agent finalized successfully.');
    }
}
