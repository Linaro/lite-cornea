# GDB Proxy

Cornea can translate between the GDB remote debug protocol (over
pipe '|') with the
the gdb-proxy subcommand. This command causes cornea to act as a
bridge, allowing gdb to control and debug an FVP as it normally would.

```
$ gdb -q
(gdb) set architecture aarch64
The target architecture is set to "aarch64".
(gdb) add-symbol-file ~/src/c/tf-a/build/GENERATED/fvp-tc2-tbb_fvp-linux.tc-fip.tc-tc2-debug/artefacts/debug/bl31.elf
add symbol table from file "/home/jimbri01/src/c/tf-a/build/GENERATED/fvp-tbb-mbedtls-ecdsa_fvp-tbb_fvp-tftf.fwu-aemv8a/artefacts/debug/bl31.elf"
(y or n) y
Reading symbols from /home/jimbri01/src/c/tf-a/build/GENERATED/fvp-tbb-mbedtls-ecdsa_fvp-tbb_fvp-tftf.fwu-aemv8a/artefacts/debug/bl31.elf...
(gdb) target remote | cornea gdb-proxy component.TC2.css.cluster0.subcluster0.cpu0
(gdb)
```

After an initialization sequence, such as the one above, gdb may be
used to debug the processor selected as the single instance argument.
To simplify usage, you can use `env` and `gdb` to create an executable
script of gdb commands that will act as if they were entered into the
prompt on startup. For example, the below script does the same as the
interactive snippet above, allowing you to skip typing or copying the
commands.

```
#!/usr/bin/env -S gdb -q -ix
set architecture aarch64
add-symbol-file ~/src/c/tf-a/build/GENERATED/fvp-tc2-tbb_fvp-linux.tc-fip.tc-tc2-debug/artefacts/debug/bl31.elf
target remote | cornea gdb-proxy component.TC2.css.cluster0.subcluster0.cpu0
```
