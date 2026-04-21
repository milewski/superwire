<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Prism\Prism\Enums\Provider;
use Prism\Prism\PrismManager;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\Runtime;
use Superwire\Laravel\Tests\Fakes\ToolLoopProvider;

final class MainTest extends TestCase
{
    public function test_demo(): void
    {
        $provider = new ToolLoopProvider([
            'generate a sequence of numbers from 1 to 5.' => [ 1, 2, 3, 4, 5 ],
            'spell out this number 1.' => 'one',
            'spell out this number 2.' => 'two',
            'spell out this number 3.' => 'three',
            'spell out this number 4.' => 'four',
            'spell out this number 5.' => 'five',
        ]);

        app()->instance(PrismManager::class, new class(app(), $provider) extends PrismManager {
            public function __construct($app, private readonly ToolLoopProvider $provider)
            {
                parent::__construct($app);
            }

            public function resolve(Provider|string $name, array $providerConfig = []): \Prism\Prism\Providers\Provider
            {
                return $this->provider;
            }
        });

        $definition = WorkflowDefinition::fromJson(file_get_contents(__DIR__ . '/minimum.json'));
        $runtime = new Runtime($definition);
        $result = $runtime->run();

        $this->assertSame([
            'numbers' => [ 'one', 'two', 'three', 'four', 'five' ],
        ], $result);

//        Workflow::fromFile()
//            ->withTools([])
//            ->withInputs([ 'question' => 'what changed?' ])
//            ->withSecrets([ 'token' => 'token-value' ])
//            ->run();
    }
}
