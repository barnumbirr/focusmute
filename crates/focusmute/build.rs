fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon-live.ico");
        res.compile().expect("failed to compile Windows resources");
    }
}
