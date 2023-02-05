host:
	#!/usr/bin/env -S gdb -ix
	set architecture aarch64
	add-symbol-file ~/src/c/tf-a/build/GENERATED/fvp-tc2-tbb_fvp-linux.tc-fip.tc-tc2-debug/artefacts/debug/bl31.elf
	add-symbol-file ~/src/c/tf-a/build/GENERATED/fvp-tc2-tbb_fvp-linux.tc-fip.tc-tc2-debug/artefacts/debug/bl1.elf
	add-symbol-file ~/src/c/tf-a/build/GENERATED/fvp-tc2-tbb_fvp-linux.tc-fip.tc-tc2-debug/artefacts/debug/bl2.elf
	target remote | cargo run -- gdb-proxy component.TC2.css.cluster0.subcluster0.cpu0
	
rss:
	#!/usr/bin/env -S gdb -ix
	set arch armv6-m
	add-symbol-file ~/src/c/tf-a/rss/build/bin/bl1_1.elf
	add-symbol-file ~/src/c/tf-a/rss/build/bin/bl1_2.elf
	add-symbol-file ~/src/c/tf-a/rss/build/bin/bl2.elf
	add-symbol-file ~/src/c/tf-a/rss/build/bin/tfm_s.elf
	add-symbol-file ~/src/c/tf-a/rss/build/bin/tfm_ns.elf
	target remote | cargo run -- gdb-proxy component.TC2.css.rss.cpu
	
@fvp-port:
         lsof -c FVP -P | awk '$5 == "IPv4" { split($9, s, ":"); if (s[1] == "localhost") print s[2]; exit }'
