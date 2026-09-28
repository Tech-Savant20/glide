// Compiles the settings UI and embeds the icon, version information and
// application manifest into glide.exe.

use std::path::PathBuf;

include!("src/icon_art.rs");

fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/settings.slint", config).expect("Slint build failed");

    println!("cargo:rerun-if-changed=src/icon_art.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = out.join("glide.ico");
    std::fs::write(&ico, icon_file(&[16, 20, 24, 32, 40, 48, 64, 256])).unwrap();

    let version = env!("CARGO_PKG_VERSION");
    let numeric: Vec<&str> = version
        .split(['.', '-', '+'])
        .take(3)
        .chain(["0"])
        .collect();
    let manifest = MANIFEST.replace("{version}", &numeric.join("."));

    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico.to_str().unwrap())
        .set_manifest(&manifest)
        .set("ProductName", "Glide")
        .set("FileDescription", "Glide - smooth scrolling for Windows")
        .set("CompanyName", "Glide contributors")
        .set(
            "LegalCopyright",
            "Copyright (c) 2026 Glide contributors. MIT OR Apache-2.0.",
        )
        .set("OriginalFilename", "glide.exe")
        .set("InternalName", "glide");
    res.compile()
        .expect("couldn't embed the icon and version info");
}

/// Runs as the signed-in user (never elevated), declares Windows 10/11, and is
/// per-monitor DPI aware so the tray icon and hook coordinates match on scaled
/// displays.
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
  <assemblyIdentity type="win32" name="Glide" version="{version}"/>
  <description>Glide: smooth scrolling for Windows</description>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v2">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <asmv3:application>
    <asmv3:windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </asmv3:windowsSettings>
  </asmv3:application>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;
