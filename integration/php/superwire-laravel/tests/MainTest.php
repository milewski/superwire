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
            'spell out this number: 1.' => 'one',
            'spell out this number: 2.' => 'two',
            'spell out this number: 3.' => 'three',
            'spell out this number: 4.' => 'four',
            'spell out this number: 5.' => 'five',
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
        $runtime = (new Runtime($definition))
            ->withInputs([
                'min' => 1,
                'max' => 5,
            ])
            ->withSecrets([
                'model' => 'qwen3.5-9b',
            ]);

        $result = $runtime->run();

        $this->assertSame([
            'numbers' => [ 'one', 'two', 'three', 'four', 'five' ],
        ], $result->output);

        $this->assertSame([1, 2, 3, 4, 5], $result->agents['numbers']->output);
        $this->assertSame('generate a sequence of numbers from 1 to 5.', $result->agents['numbers']->messages[0]['content']);
        $this->assertSame(['one', 'two', 'three', 'four', 'five'], $result->agents['counter']->output);
        $this->assertCount(5, $result->agents['counter']->iterations);
        $this->assertSame('spell out this number: 1.', $result->agents['counter']->iterations[0]->messages[0]['content']);
    }

    public function test_runtime_validates_required_inputs_and_secrets(): void
    {
        $definition = WorkflowDefinition::fromJson(file_get_contents(__DIR__ . '/minimum.json'));

        $this->expectException(
            \InvalidArgumentException::class,
        );

        (new Runtime($definition))
            ->withInputs([
                'min' => 1,
            ])
            ->run();

//        Workflow::fromFile()
//            ->withTools([])
//            ->withInputs([ 'question' => 'what changed?' ])
//            ->withSecrets([ 'token' => 'token-value' ])
//            ->run();
    }
}
