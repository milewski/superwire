<?php

declare(strict_types=1);

namespace Superwire\Contracts\Tests\Stages;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Support\Stages\CompletionToolLoopStage;

final class CompletionToolLoopStageTest extends TestCase
{
    public function testItPrefersRuntimeToolCallsOverFinalizeCallsInSameTurn(): void
    {
        $decision = (new CompletionToolLoopStage())->decide([
            ['id' => '1', 'name' => 'finalize_success', 'arguments' => ['answer' => 'done']],
            ['id' => '2', 'name' => 'search_docs', 'arguments' => ['query' => 'x']],
        ]);

        self::assertSame('continue', $decision['status']);
        self::assertCount(1, $decision['runtime_tool_calls']);
        self::assertSame('search_docs', $decision['runtime_tool_calls'][0]['name']);
    }

    public function testItExtractsFinalizeSuccessPayload(): void
    {
        $decision = (new CompletionToolLoopStage())->decide([
            ['id' => '3', 'name' => 'finalize_success', 'arguments' => ['answer' => ['summary' => 'ok']]],
        ]);

        self::assertSame('success', $decision['status']);
        self::assertSame(['summary' => 'ok'], $decision['output']);
    }
}
