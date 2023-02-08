# Memory

Cornea reads memory from the perspective of an instance with
the memory-read subcommand. This command creates human-readable,
compact hex-dumps and can group memory locations into various word
sizes.

For example, to read 8 bytes of memory from flash0, starting at
address 6, grouped into 2-byte/u16 chunks:
```
$ cornea memory-read component.IoT_Corstone_1000.board.flash0 6 8 --group-by u16
         0    2    4    6    8    a    c    e
00000000                e800 0010 e7ff e800            ........
```

As another example, on another platform, the following reads 2
32-bit words as seen by the rss cpu, starting at 0x310000a8:

```
$ cornea memory-read component.TC2.css.rss.cpu 310000a8 8 --group-by u32
         0        4        8        c
310000a0                   5b2e3408 6e72ea2f         .4.[/.rn
```
