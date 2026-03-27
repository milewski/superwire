"use strict";

const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const packageDirectory = path.resolve(__dirname, "..");
const workspaceRootDirectory = path.resolve(packageDirectory, "..", "..", "..");

const nativeLibraryDirectory = path.resolve(packageDirectory, "native");
const sourceLibraryPath = path.resolve(
  workspaceRootDirectory,
  "target",
  "release",
  libraryFileNameForCurrentPlatform(),
);
const destinationLibraryPath = path.resolve(nativeLibraryDirectory, libraryFileNameForCurrentPlatform());

buildRustFfiLibrary();
copyNativeLibrary();

function buildRustFfiLibrary() {
  execFileSync("cargo", ["build", "-p", "ffi", "--release"], {
    cwd: workspaceRootDirectory,
    stdio: "inherit",
  });
}

function copyNativeLibrary() {
  if (!fs.existsSync(sourceLibraryPath)) {
    throw new Error(`Rust cdylib was not produced at ${sourceLibraryPath}`);
  }

  fs.mkdirSync(nativeLibraryDirectory, { recursive: true });
  fs.copyFileSync(sourceLibraryPath, destinationLibraryPath);

  process.stdout.write(`Copied ${sourceLibraryPath} -> ${destinationLibraryPath}\n`);
}

function libraryFileNameForCurrentPlatform() {
  switch (process.platform) {
    case "darwin":
      return "libffi.dylib";

    case "linux":
      return "libffi.so";

    case "win32":
      return "ffi.dll";

    default:
      throw new Error(`Unsupported platform for engine ffi library: ${process.platform}`);
  }
}
