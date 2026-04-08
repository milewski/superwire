<?php

namespace Superwire\Laravel\Schema;

use JsonException;
use Swaggest\JsonSchema\Schema;

final readonly class JsonSchemaBuilder
{
    private function __construct(
        private Schema $schema,
    )
    {
    }

    public static function object(): self
    {
        return new self(Schema::object());
    }

    public static function string(): self
    {
        return new self(Schema::string());
    }

    public static function nullableString(): self
    {
        $schema = Schema::string();
        $schema->type = [ 'string', 'null' ];

        return new self($schema);
    }

    public function property(string $name, self $propertySchema): self
    {
        $this->schema->setProperty($name, $propertySchema->schema);

        return $this;
    }

    /**
     * @param list<string> $requiredProperties
     */
    public function required(array $requiredProperties): self
    {
        $this->schema->required = $requiredProperties;

        return $this;
    }

    /**
     * @return array<string, mixed>
     * @throws JsonException
     */
    public function toArray(): array
    {
        $serializedSchema = json_encode($this->schema, JSON_THROW_ON_ERROR);

        return json_decode($serializedSchema, true, 512, JSON_THROW_ON_ERROR);
    }
}
