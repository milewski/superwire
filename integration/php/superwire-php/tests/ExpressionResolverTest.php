<?php

declare(strict_types=1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Exception\ExpressionResolutionException;
use Superwire\Contracts\Support\ExpressionResolver;

final class ExpressionResolverTest extends TestCase
{
    public function testItResolvesReferencesAndTemplatesFromRuntimeContext(): void
    {
        $resolver = new ExpressionResolver();

        $resolvedTemplate = $resolver->resolve(
            [
                '$template' => [
                    'Hello ',
                    ['$expr' => ['$ref' => 'input.name']],
                    ', summary=',
                    ['$expr' => ['$ref' => 'agent.summary']],
                ],
            ],
            [
                'input' => ['name' => 'Rafael'],
                'agent' => ['summary' => 'ready'],
            ]
        );

        self::assertSame('Hello Rafael, summary=ready', $resolvedTemplate);
    }

    public function testItThrowsForMissingReferenceRoots(): void
    {
        $resolver = new ExpressionResolver();

        $this->expectException(ExpressionResolutionException::class);

        $resolver->resolve(['$ref' => 'missing.value'], ['input' => []]);
    }
}
