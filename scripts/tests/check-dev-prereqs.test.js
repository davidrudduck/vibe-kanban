const assert = require("node:assert/strict");
const test = require("node:test");

const {
  checkDevPrereqs,
  detectPackageManager,
} = require("../check-dev-prereqs");

function env({ commands = [], paths = [], clangHeaders = true } = {}) {
  const commandSet = new Set(commands);
  const pathSet = new Set(paths);

  return {
    platform: "linux",
    hasCommand: (command) => commandSet.has(command),
    exists: (filePath) => pathSet.has(filePath),
    hasClangHeaders: () => clangHeaders,
    env: {},
  };
}

test("passes on non-Linux platforms", () => {
  const result = checkDevPrereqs({ platform: "darwin" });

  assert.equal(result.ok, true);
  assert.deepEqual(result.errors, []);
});

test("reports missing bindgen prerequisites with apt advice", () => {
  const result = checkDevPrereqs(
    env({
      commands: ["cargo", "cargo-watch", "apt-get"],
      paths: ["/etc/debian_version"],
      clangHeaders: false,
    }),
  );

  assert.equal(result.ok, false);
  assert.match(result.errors.join("\n"), /clang\/libclang development headers/);
  assert.match(result.warnings.join("\n"), /pkg-config/);
  assert.match(result.warnings.join("\n"), /SQLite development headers/);
  assert.match(result.errors.join("\n"), /sudo apt-get update/);
  assert.match(result.errors.join("\n"), /stdarg\.h file not found/);
});

test("accepts explicit bindgen header workaround", () => {
  const result = checkDevPrereqs({
    ...env({
      commands: ["cargo", "cargo-watch"],
      clangHeaders: false,
    }),
    env: {
      BINDGEN_EXTRA_CLANG_ARGS:
        "-isystem /usr/lib/gcc/x86_64-linux-gnu/15/include",
    },
  });

  assert.equal(result.ok, true);
  assert.deepEqual(result.errors, []);
});

test("accepts installed prerequisites", () => {
  const result = checkDevPrereqs(
    env({
      commands: ["cargo", "cargo-watch", "clang", "pkg-config"],
      paths: ["/usr/include/sqlite3.h"],
      clangHeaders: true,
    }),
  );

  assert.equal(result.ok, true);
  assert.deepEqual(result.errors, []);
});

test("detects Linux package managers", () => {
  assert.equal(
    detectPackageManager({
      platform: "linux",
      exists: () => false,
      hasCommand: (command) => command === "dnf",
    }).name,
    "dnf",
  );
});
