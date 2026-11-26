/// The app manifest tauri-build writes by default (common controls v6), plus
/// the Windows 10/11 compatibility entry: without it Windows treats the app as
/// pre-Windows 8, where a layered window cannot be a child window, and the
/// taskbar button (taskbar.rs) would not be created.
const WINDOWS_MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>"#;

fn main() {
    // tauri::generate_context!() embeds ../dist at compile time.
    println!("cargo:rerun-if-changed=../dist");
    let windows = tauri_build::WindowsAttributes::new().app_manifest(WINDOWS_MANIFEST);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}
