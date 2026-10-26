use tsrs_core::tspath::Path;

use tsrs_execute::build::buildtask::could_change_resolution;

#[test]
fn paths_module_resolution_could_find() {
    for path in
        ["/a/b.ts", "/a/b.d.ts", "/a/b.tsx", "/a/b.mts", "/a/b.cjs", "/a/b.json", "/a/package.json"]
    {
        assert!(could_change_resolution(&Path::new(path)), "{path}");
    }
    for path in ["/a/b.md", "/a/b.css", "/a/b.tsbuildinfo", "/a/b.js.map", "/a/4913"] {
        assert!(!could_change_resolution(&Path::new(path)), "{path}");
    }
}
