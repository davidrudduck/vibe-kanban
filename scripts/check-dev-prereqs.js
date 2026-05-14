#!/usr/bin/env node

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

function commandExists(command) {
  const result = spawnSync("sh", ["-c", `command -v ${command}`], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });

  return result.status === 0;
}

function hasAnyPath(paths, exists = fs.existsSync) {
  return paths.some((candidate) => exists(candidate));
}

function findFiles(root, predicate, limit = 1) {
  const matches = [];
  const pending = [root];

  while (pending.length > 0 && matches.length < limit) {
    const current = pending.pop();
    let entries;

    try {
      entries = fs.readdirSync(current, { withFileTypes: true });
    } catch {
      continue;
    }

    for (const entry of entries) {
      const fullPath = path.join(current, entry.name);
      if (entry.isDirectory()) {
        pending.push(fullPath);
      } else if (predicate(fullPath)) {
        matches.push(fullPath);
        if (matches.length >= limit) {
          break;
        }
      }
    }
  }

  return matches;
}

function hasClangResourceHeaders({
  exists = fs.existsSync,
  find = findFiles,
} = {}) {
  const knownPaths = [
    "/usr/lib/clang",
    "/usr/lib/llvm",
    "/usr/local/lib/clang",
    "/opt/homebrew/opt/llvm/lib/clang",
    "/home/linuxbrew/.linuxbrew/opt/llvm/lib/clang",
  ];

  if (
    knownPaths.some((candidate) => {
      if (!exists(candidate)) {
        return false;
      }

      return (
        find(candidate, (filePath) => filePath.endsWith("/include/stdarg.h"))
          .length > 0
      );
    })
  ) {
    return true;
  }

  return false;
}

function detectPackageManager({
  platform = process.platform,
  exists = fs.existsSync,
  hasCommand = commandExists,
} = {}) {
  if (platform !== "linux") {
    return null;
  }

  if (hasCommand("apt-get") || exists("/etc/debian_version")) {
    return {
      name: "apt",
      install:
        "sudo apt-get update && sudo apt-get install -y clang libclang-dev pkg-config libsqlite3-dev",
    };
  }

  if (hasCommand("dnf")) {
    return {
      name: "dnf",
      install:
        "sudo dnf install clang clang-devel pkgconf-pkg-config sqlite-devel",
    };
  }

  if (hasCommand("pacman")) {
    return {
      name: "pacman",
      install: "sudo pacman -S clang pkgconf sqlite",
    };
  }

  if (hasCommand("apk")) {
    return {
      name: "apk",
      install: "sudo apk add clang clang-dev pkgconf sqlite-dev",
    };
  }

  return null;
}

function checkDevPrereqs({
  platform = process.platform,
  exists = fs.existsSync,
  hasCommand = commandExists,
  hasClangHeaders = hasClangResourceHeaders,
  env = process.env,
} = {}) {
  if (platform !== "linux") {
    return { ok: true, warnings: [], errors: [] };
  }

  const missing = [];
  const warnings = [];

  if (!hasCommand("cargo")) {
    missing.push("cargo");
  }

  if (!hasCommand("cargo-watch")) {
    missing.push("cargo-watch");
  }

  if (
    !env.BINDGEN_EXTRA_CLANG_ARGS &&
    !hasCommand("clang") &&
    !hasClangHeaders()
  ) {
    missing.push("clang/libclang development headers");
  }

  if (!hasCommand("pkg-config")) {
    warnings.push(
      "pkg-config is not installed. It is recommended for native dependency discovery on Linux.",
    );
  }

  if (
    !hasAnyPath(
      ["/usr/include/sqlite3.h", "/usr/local/include/sqlite3.h"],
      exists,
    )
  ) {
    warnings.push(
      "SQLite development headers were not found in standard system include paths. They are recommended for Linux development setups.",
    );
  }

  if (!env.BINDGEN_EXTRA_CLANG_ARGS && !hasClangHeaders()) {
    warnings.push(
      'bindgen may not find C standard headers. Installing clang/libclang-dev is preferred; as a local workaround, set BINDGEN_EXTRA_CLANG_ARGS="-isystem $(gcc -print-file-name=include)".',
    );
  }

  if (missing.length === 0) {
    return { ok: true, warnings, errors: [] };
  }

  const packageManager = detectPackageManager({ platform, exists, hasCommand });
  const installAdvice = packageManager
    ? packageManager.install
    : "Install clang, libclang development headers, pkg-config, and SQLite development headers with your Linux distribution package manager.";

  return {
    ok: false,
    warnings,
    errors: [
      "Missing Linux development prerequisites for `pnpm run dev`:",
      ...missing.map((item) => `  - ${item}`),
      "",
      "`pnpm run dev` builds Rust crates that use SQLx SQLite preupdate hooks. That path runs bindgen through libclang; without Clang development headers it can fail with `fatal error: stdarg.h file not found` while compiling `libsqlite3-sys`.",
      "",
      `Install the missing packages, for example:\n  ${installAdvice}`,
      "",
      "If your distro already has libclang but bindgen cannot find C headers, this local workaround also unblocks the build:",
      '  export BINDGEN_EXTRA_CLANG_ARGS="-isystem $(gcc -print-file-name=include)"',
    ],
  };
}

function main() {
  const result = checkDevPrereqs();

  for (const warning of result.warnings) {
    console.warn(`Warning: ${warning}`);
  }

  if (!result.ok) {
    console.error(result.errors.join("\n"));
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}

module.exports = {
  checkDevPrereqs,
  detectPackageManager,
  hasClangResourceHeaders,
};
