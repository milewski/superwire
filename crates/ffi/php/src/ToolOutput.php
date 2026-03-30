<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

use Spatie\LaravelData\Attributes\MapOutputName;
use Spatie\LaravelData\Data;
use Spatie\LaravelData\Mappers\SnakeCaseMapper;

#[MapOutputName(SnakeCaseMapper::class)]
abstract class ToolOutput extends Data
{
}
