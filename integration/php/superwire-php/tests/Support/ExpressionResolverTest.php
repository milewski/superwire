<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Exception\ExpressionResolutionException;
use Superwire\Contracts\Support\ExpressionResolver;

final class ExpressionResolverTest extends TestCase
{
    public function test_it_resolves_references_and_templates_from_runtime_context(): void
    {
        $resolver = new ExpressionResolver();

        $resolvedTemplate = $resolver->resolve(
            expression: [
                '$template' => [
                    'Hello ',
                    [ '$expr' => [ '$ref' => 'input.name' ] ],
                    ', summary=',
                    [ '$expr' => [ '$ref' => 'agent.summary' ] ],
                ],
            ],
            runtimeContext: [
                'input' => [ 'name' => 'Rafael' ],
                'agent' => [ 'summary' => 'ready' ],
            ],
        );

        $this->assertSame('Hello Rafael, summary=ready', $resolvedTemplate);
    }

    public function test_it_throws_for_missing_reference_roots(): void
    {
        $resolver = new ExpressionResolver();

        $this->expectException(ExpressionResolutionException::class);

        $resolver->resolve([ '$ref' => 'missing.value' ], [ 'input' => [] ]);
    }
}
