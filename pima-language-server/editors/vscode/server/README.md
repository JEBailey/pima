# Bundled language servers

Release packaging places one optimized server executable in the directory for
its target platform:

```text
server/win32-x64/pima-language-server.exe
server/linux-x64/pima-language-server
server/darwin-x64/pima-language-server
server/darwin-arm64/pima-language-server
```

Development builds continue to use the workspace `target/debug` executable
when no bundled server is present.
