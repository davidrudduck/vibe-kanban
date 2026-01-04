use std::process::Command;

fn main() {
    // Git commit (short hash)
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        && output.status.success()
    {
        let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
        println!("cargo:rustc-env=VK_GIT_COMMIT={}", commit);
    }

    // Git branch
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        && output.status.success()
    {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        println!("cargo:rustc-env=VK_GIT_BRANCH={}", branch);
    }

    // Build timestamp (ISO 8601 format)
    if let Ok(output) = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        && output.status.success()
    {
        let timestamp = String::from_utf8_lossy(&output.stdout).trim().to_string();
        println!("cargo:rustc-env=VK_BUILD_TIMESTAMP={}", timestamp);
    }
}
