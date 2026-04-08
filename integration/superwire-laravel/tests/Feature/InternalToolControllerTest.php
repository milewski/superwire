<?php

namespace Superwire\Laravel\Tests\Feature;

use Superwire\Laravel\Tests\Fixtures\EchoTool;
use Superwire\Laravel\Tests\TestCase;

final class InternalToolControllerTest extends TestCase
{
    protected function defineEnvironment($application): void
    {
        parent::defineEnvironment($application);

        $application['config']->set('superwire.tools.registered_classes', [EchoTool::class]);
    }

    public function testExecutesRegisteredToolWhenRequestIsAuthorized(): void
    {
        $response = $this
            ->withHeaders([
                'x-superwire-internal-token' => 'test-internal-token',
            ])
            ->postJson('/superwire/tools/echo_tool/execute', [
                'agent_input' => [
                    'city' => 'Lisbon',
                ],
                'bound_input' => [
                    'units' => 'metric',
                ],
            ]);

        $response
            ->assertOk()
            ->assertJson([
                'agent_input' => [
                    'city' => 'Lisbon',
                ],
                'bound_input' => [
                    'units' => 'metric',
                ],
            ]);
    }

    public function testRejectsRequestWhenTokenIsInvalid(): void
    {
        $response = $this
            ->withHeaders([
                'x-superwire-internal-token' => 'invalid-token',
            ])
            ->postJson('/superwire/tools/echo_tool/execute', [
                'agent_input' => [],
                'bound_input' => [],
            ]);

        $response->assertForbidden();
    }

    public function testRejectsRemoteAddressWhenLocalhostEnforcementIsEnabled(): void
    {
        config()->set('superwire.security.enforce_localhost_only', true);

        $response = $this
            ->withServerVariables([
                'REMOTE_ADDR' => '10.10.10.10',
            ])
            ->withHeaders([
                'x-superwire-internal-token' => 'test-internal-token',
            ])
            ->postJson('/superwire/tools/echo_tool/execute', [
                'agent_input' => [],
                'bound_input' => [],
            ]);

        $response->assertForbidden();
    }
}
