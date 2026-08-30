/* Script de link do kernel Nexo — higher half, code model "kernel".
 * Cada seção começa em página própria para que o loader aplique W^X por segmento.
 * Símbolos __*_start/__*_end são usados pelo kernel para verificar permissões. */
ENTRY(_start)
OUTPUT_FORMAT(elf64-x86-64)

KERNEL_VMA = 0xffffffff80000000;

/* Segmentos explícitos: o loader aplica exatamente estas permissões (W^X). */
PHDRS
{
    text   PT_LOAD FLAGS(5); /* R X */
    rodata PT_LOAD FLAGS(4); /* R   */
    data   PT_LOAD FLAGS(6); /* R W */
}

SECTIONS
{
    . = KERNEL_VMA;
    __kernel_start = .;

    .text ALIGN(4K) : {
        __text_start = .;
        *(.text .text.*)
        . = ALIGN(4K);
        __text_end = .;
    } :text

    .rodata ALIGN(4K) : {
        __rodata_start = .;
        *(.rodata .rodata.*)
        *(.data.rel.ro .data.rel.ro.*)
        . = ALIGN(4K);
        __rodata_end = .;
    } :rodata

    .data ALIGN(4K) : {
        __data_start = .;
        *(.data .data.*)
        *(.got .got.plt)
        . = ALIGN(4K);
        __data_end = .;
    } :data

    .bss ALIGN(4K) : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4K);
        __bss_end = .;
    } :data

    __kernel_end = .;

    /DISCARD/ : {
        *(.eh_frame .eh_frame_hdr)
        *(.note .note.*)
        *(.comment)
    }
}
