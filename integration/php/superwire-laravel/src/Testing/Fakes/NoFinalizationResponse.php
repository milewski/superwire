<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Testing\Fakes;

final readonly class NoFinalizationResponse
{
    public function __construct(
        public string $text,
    )
    {
    }
}
