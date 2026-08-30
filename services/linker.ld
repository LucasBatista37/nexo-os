/* Programa de usuario: base baixa, segmentos W^X separados em paginas proprias. */
ENTRY(_start)
OUTPUT_FORMAT(elf64-x86-64)

PHDRS
{
    text   PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data   PT_LOAD FLAGS(6);
}

SECTIONS
{
    . = 0x400000;
    .text ALIGN(4K) : { *(.text .text.*) . = ALIGN(4K); } :text
    .rodata ALIGN(4K) : { *(.rodata .rodata.*) *(.data.rel.ro .data.rel.ro.*) . = ALIGN(4K); } :rodata
    .data ALIGN(4K) : { *(.data .data.*) *(.got .got.plt) . = ALIGN(4K); } :data
    .bss ALIGN(4K) : { *(.bss .bss.*) *(COMMON) . = ALIGN(4K); } :data
    /DISCARD/ : { *(.eh_frame .eh_frame_hdr) *(.note .note.*) *(.comment) }
}
