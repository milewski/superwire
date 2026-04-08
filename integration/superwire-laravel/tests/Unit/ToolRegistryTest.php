<?php

namespace Superwire\Laravel\Tests\Unit;

use JsonException;
use Superwire\Laravel\Exceptions\InvalidToolClassException;
use Superwire\Laravel\Support\ToolRegistry;
use Superwire\Laravel\Tests\Fixtures\EchoTool;
use Superwire\Laravel\Tests\TestCase;

final class ToolRegistryTest extends TestCase
{
    public function testResolvesToolClassFromConfiguredRegistry(): void
    {
        config()->set('superwire.tools.registered_classes', [ EchoTool::class ]);

        $resolvedToolClass = app(ToolRegistry::class)->resolveToolClass('echo_tool');

        $this->assertSame(EchoTool::class, $resolvedToolClass);
    }

    /**
     * @throws JsonException
     */
    public function testResolvesToolClassFromBuildManifest(): void
    {
        $buildDirectory = $this->createTemporaryDirectory('superwire-build');
        $manifestPath = $buildDirectory . DIRECTORY_SEPARATOR . 'tool-registry.json';

        config()->set('superwire.build.root_directory', $buildDirectory);
        config()->set('superwire.tools.registered_classes', []);

        file_put_contents($manifestPath, json_encode([
            'tools' => [
                'echo_tool' => [
                    'class' => EchoTool::class,
                ],
            ],
        ], JSON_THROW_ON_ERROR));

        $resolvedToolClass = app(ToolRegistry::class)->resolveToolClass('echo_tool');

        $this->assertSame(EchoTool::class, $resolvedToolClass);
    }

    public function testThrowsWhenToolCannotBeResolved(): void
    {
        config()->set('superwire.tools.registered_classes', []);

        $this->expectException(InvalidToolClassException::class);

        app(ToolRegistry::class)->resolveToolClass('unknown_tool');
    }
}
