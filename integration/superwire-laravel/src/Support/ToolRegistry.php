<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Support;

use Illuminate\Contracts\Config\Repository;
use Superwire\Laravel\Contracts\Tool;
use Superwire\Laravel\Exceptions\InvalidToolClassException;

final class ToolRegistry
{
    public function __construct(private readonly Repository $config)
    {
    }

    /**
     * @return class-string<Tool>
     */
    public function resolveToolClass(string $toolName): string
    {
        $manifestPath = rtrim((string) $this->config->get('superwire.build.root_directory'), DIRECTORY_SEPARATOR)
            . DIRECTORY_SEPARATOR
            . 'tool-registry.json';

        if (is_file($manifestPath)) {

            $manifestPayload = json_decode((string) file_get_contents($manifestPath), true);

            if (is_array($manifestPayload)
                && isset($manifestPayload[ 'tools' ][ $toolName ][ 'class' ])
                && is_string($manifestPayload[ 'tools' ][ $toolName ][ 'class' ])
            ) {
                return $manifestPayload[ 'tools' ][ $toolName ][ 'class' ];
            }

        }

        $configuredToolClasses = $this->config->get('superwire.tools.registered_classes', []);

        foreach ($configuredToolClasses as $configuredToolClass) {

            if (!is_string($configuredToolClass)) {
                continue;
            }

            if (!is_subclass_of($configuredToolClass, Tool::class)) {
                continue;
            }

            if ($configuredToolClass::name() === $toolName) {
                return $configuredToolClass;
            }

        }

        throw new InvalidToolClassException(sprintf('failed to resolve superwire tool class for `%s`', $toolName));
    }
}
