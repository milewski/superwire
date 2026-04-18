<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support;

use Superwire\Contracts\Exception\ExpressionResolutionException;

final class ExpressionResolver
{
    /**
     * @param array<string, mixed> $runtimeContext
     */
    public function resolve(mixed $expression, array $runtimeContext): mixed
    {
        if (!is_array($expression)) {
            return $expression;
        }

        if (array_key_exists('$ref', $expression)) {

            if (!is_string($expression[ '$ref' ])) {
                throw new ExpressionResolutionException('reference expressions require string `$ref` values');
            }

            return $this->resolveReferencePath($expression[ '$ref' ], $runtimeContext);

        }

        if (array_key_exists('$template', $expression)) {

            if (!is_array($expression[ '$template' ])) {
                throw new ExpressionResolutionException('template expressions require array `$template` values');
            }

            return $this->resolveTemplate($expression[ '$template' ], $runtimeContext);

        }

        if (array_key_exists('$expr', $expression)) {
            return $this->resolve($expression[ '$expr' ], $runtimeContext);
        }

        if (array_key_exists('$call', $expression)) {
            return $this->resolveCall($expression, $runtimeContext);
        }

        $resolvedObject = [];

        foreach ($expression as $fieldName => $fieldValue) {
            $resolvedObject[ $fieldName ] = $this->resolve($fieldValue, $runtimeContext);
        }

        return $resolvedObject;
    }

    /**
     * @param array<string, mixed> $runtimeContext
     */
    public function resolveReferencePath(string $referencePath, array $runtimeContext): mixed
    {
        $pathSegments = explode('.', $referencePath);
        $rootSegment = array_shift($pathSegments);

        if ($rootSegment === null || $rootSegment === '') {
            throw new ExpressionResolutionException("invalid reference path `{$referencePath}`");
        }

        if (!array_key_exists($rootSegment, $runtimeContext)) {
            throw new ExpressionResolutionException("unknown reference root `{$rootSegment}` in `{$referencePath}`");
        }

        $currentValue = $runtimeContext[ $rootSegment ];

        foreach ($pathSegments as $pathSegment) {

            if (is_array($currentValue) && array_key_exists($pathSegment, $currentValue)) {

                $currentValue = $currentValue[ $pathSegment ];

                continue;

            }

            throw new ExpressionResolutionException("unable to resolve `{$referencePath}`");

        }

        return $currentValue;
    }

    /**
     * @param list<mixed> $templateParts
     * @param array<string, mixed> $runtimeContext
     */
    private function resolveTemplate(array $templateParts, array $runtimeContext): string
    {
        $resolvedTemplate = '';

        foreach ($templateParts as $templatePart) {

            $resolvedPart = $this->resolve($templatePart, $runtimeContext);

            if (is_scalar($resolvedPart) || $resolvedPart === null) {

                $resolvedTemplate .= (string) $resolvedPart;

                continue;

            }

            $encodedPart = json_encode($resolvedPart, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES);

            if ($encodedPart === false) {
                throw new ExpressionResolutionException('failed to encode template interpolation value');
            }

            $resolvedTemplate .= $encodedPart;

        }

        return $resolvedTemplate;
    }

    /**
     * @param array<string, mixed> $callExpression
     * @param array<string, mixed> $runtimeContext
     * @return array<string, mixed>
     */
    private function resolveCall(array $callExpression, array $runtimeContext): array
    {
        $callName = $callExpression[ '$call' ] ?? null;

        if (!is_string($callName)) {
            throw new ExpressionResolutionException('call expressions require string `$call` values');
        }

        $resolvedArguments = [];

        if (array_key_exists('args', $callExpression)) {

            if (!is_array($callExpression[ 'args' ])) {
                throw new ExpressionResolutionException('call expressions require array `args` values when present');
            }

            $resolvedArguments = array_map(
                fn (mixed $argumentValue): mixed => $this->resolve($argumentValue, $runtimeContext),
                array_values($callExpression[ 'args' ]),
            );

        }

        $resolvedNamedArguments = [];

        if (array_key_exists('named', $callExpression)) {

            if (!is_array($callExpression[ 'named' ])) {
                throw new ExpressionResolutionException('call expressions require object `named` values when present');
            }

            foreach ($callExpression[ 'named' ] as $argumentName => $argumentValue) {

                if (!is_string($argumentName)) {
                    throw new ExpressionResolutionException('named call argument names must be strings');
                }

                $resolvedNamedArguments[ $argumentName ] = $this->resolve($argumentValue, $runtimeContext);

            }

        }

        return [
            '$call' => $callName,
            'args' => $resolvedArguments,
            'named' => $resolvedNamedArguments,
        ];
    }
}
