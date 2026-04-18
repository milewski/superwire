<?php

declare(strict_types=1);

namespace Superwire\Laravel\Support;

use Superwire\Contracts\Contracts\AgentDriverInterface;
use Superwire\Contracts\Contracts\DriverRegistryInterface;
use Superwire\Contracts\Exception\DriverNotFoundException;

final class LaravelDriverRegistry implements DriverRegistryInterface
{
    /**
     * @var array<string, AgentDriverInterface>
     */
    private array $driversByName = [];

    public function register(string $driverName, AgentDriverInterface $driver): void
    {
        $normalizedDriverName = trim(strtolower($driverName));
        $this->driversByName[$normalizedDriverName] = $driver;
    }

    public function has(string $driverName): bool
    {
        $normalizedDriverName = trim(strtolower($driverName));

        return array_key_exists($normalizedDriverName, $this->driversByName);
    }

    public function get(string $driverName): AgentDriverInterface
    {
        $normalizedDriverName = trim(strtolower($driverName));

        if ($this->has($normalizedDriverName)) {
            return $this->driversByName[$normalizedDriverName];
        }

        throw new DriverNotFoundException("driver `{$driverName}` is not registered");
    }
}
