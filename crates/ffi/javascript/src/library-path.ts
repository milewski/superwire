import fs from 'node:fs'
import path from 'node:path'

export function resolveDefaultLibraryPath(): string {
    if (process.env.ENGINE_AI_FFI_LIBRARY_PATH) {
        return path.resolve(process.env.ENGINE_AI_FFI_LIBRARY_PATH)
    }

    const nativeLibraryFileName = libraryFileNameForCurrentPlatform()
    const candidatePaths = [
        path.resolve(__dirname, 'native', nativeLibraryFileName),
        path.resolve(__dirname, '..', 'native', nativeLibraryFileName),
        path.resolve(__dirname, '..', '..', '..', 'target', 'release', nativeLibraryFileName),
        path.resolve(__dirname, '..', '..', '..', '..', 'target', 'release', nativeLibraryFileName),
    ]

    for (const candidatePath of candidatePaths) {
        if (fs.existsSync(candidatePath)) {
            return candidatePath
        }
    }

    throw new Error(
        `Unable to locate native ffi library (${ nativeLibraryFileName }). Tried: ${ candidatePaths.join(', ') }. Run npm run build:native first.`,
    )
}

function libraryFileNameForCurrentPlatform(): string {
    switch (process.platform) {
        case 'darwin':
            return 'libffi.dylib'

        case 'linux':
            return 'libffi.so'

        case 'win32':
            return 'ffi.dll'

        default:
            throw new Error(`Unsupported platform for engine ffi library: ${ process.platform }`)
    }
}
