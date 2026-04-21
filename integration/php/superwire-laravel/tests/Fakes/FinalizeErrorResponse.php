<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Fakes;

final readonly class FinalizeErrorResponse
{
    public function __construct(
        public string $message,
    ) {
    }
}
