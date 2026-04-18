<?php

declare(strict_types=1);

namespace Superwire\Contracts\Contracts;

interface DriverRegistryInterface
{
    public function register(string $driverName, AgentDriverInterface $driver): void;

    public function has(string $driverName): bool;

    public function get(string $driverName): AgentDriverInterface;
}
