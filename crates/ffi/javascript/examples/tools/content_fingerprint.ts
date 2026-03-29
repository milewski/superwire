import { createHash } from 'node:crypto'

import { schema, Tool, type ToolArguments } from '../../src'

export type ContentFingerprintInput = {
    headline: string;
    summary: string;
}

export type ContentFingerprintBoundedInput = {
    collection_id: string;
    signing_salt: string;
}

export type ContentFingerprintOutput = {
    fingerprint_sha256: string;
    short_code: string;
}

type ContentFingerprintArguments = ToolArguments<ContentFingerprintInput, ContentFingerprintBoundedInput>

export class ContentFingerprint extends Tool<
    ContentFingerprintInput,
    ContentFingerprintOutput,
    ContentFingerprintBoundedInput
> {
    readonly description = 'Create a deterministic SHA-256 fingerprint for generated content'

    readonly inputSchema = schema.object({
        headline: schema.string(),
        summary: schema.string(),
    })

    readonly outputSchema = schema.object({
        fingerprint_sha256: schema.string(),
        short_code: schema.string(),
    })

    execute(toolArguments: ContentFingerprintArguments): ContentFingerprintOutput {
        const generatedInput = toolArguments.input
        const boundedInput = toolArguments.bounded
      
        const canonicalPayload = [
            boundedInput.collection_id,
            generatedInput.headline.trim(),
            generatedInput.summary.trim(),
            boundedInput.signing_salt,
        ].join('::')

        const fingerprintSha256 = createHash('sha256')
            .update(canonicalPayload)
            .digest('hex')

        const shortCode = fingerprintSha256.slice(0, 12)

        return {
            fingerprint_sha256: fingerprintSha256,
            short_code: shortCode,
        }
    }
}
