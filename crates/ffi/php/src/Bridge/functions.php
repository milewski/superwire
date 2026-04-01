<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

function createEngineFfiBridge(array $options = []): EngineFfiBridge
{
    return new EngineFfiBridge($options);
}
