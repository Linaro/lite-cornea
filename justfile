connect-corstone1000-se:
	#!/usr/bin/env -S gdb -ix
	set arch armv6-m
	add-symbol-file ~/src/c/zephyr/build/fvp_corstone1000_a35/hello_world/tfm/bin/bl1.elf
	target remote | cargo run -- -p $(just fvp-port) gdb-proxy component.IoT_Corstone_1000.se.cpu

@fvp-port:
         lsof -c FVP -P | awk '$5 == "IPv4" { split($9, s, ":"); if (s[1] == "localhost") print s[2] }'
