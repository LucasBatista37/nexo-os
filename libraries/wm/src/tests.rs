
#[test]
fn bilinear_scaling() {
    // gradiente 2x1 [0, 255] ampliado para 4x1: rampa suave com amostragem no centro
    let buf: Vec<u8> = std::vec![0, 0, 0, 0, 255, 255, 255, 0];
    let w = Window {
        rect: Rect::new(0, 0, 4, 1),
        z: 0,
        pixels: &buf,
        stride: 2,
        src_w: 2,
        src_h: 1,
        format: PixelFormat::Rgbx8888,
        alpha: 255,
    };
    let mut out_buf = std::vec![0u8; 4 * 1 * 4];
    let mut out = Surface::new(&mut out_buf, 4, 1, 4, PixelFormat::Rgbx8888).unwrap();
    composite(&mut out, &[w], Rect::new(0, 0, 4, 1), Color::rgb(9, 9, 9));
    let px = |i: usize| out_buf[i * 4];
    assert_eq!((px(0), px(1), px(2), px(3)), (0, 64, 191, 255));

    // cor solida ampliada continua solida (vizinhos iguais interpolam para o mesmo valor)
    let solid: Vec<u8> = std::iter::repeat_n([10u8, 200, 30, 0], 4).flatten().collect();
    let w = Window {
        rect: Rect::new(0, 0, 5, 5),
        z: 0,
        pixels: &solid,
        stride: 2,
        src_w: 2,
        src_h: 2,
        format: PixelFormat::Rgbx8888,
        alpha: 255,
    };
    let mut out_buf = std::vec![0u8; 5 * 5 * 4];
    let mut out = Surface::new(&mut out_buf, 5, 5, 5, PixelFormat::Rgbx8888).unwrap();
    composite(&mut out, &[w], Rect::new(0, 0, 5, 5), Color::rgb(0, 0, 0));
    for i in 0..25 {
        assert_eq!(
            (out_buf[i * 4], out_buf[i * 4 + 1], out_buf[i * 4 + 2]),
            (10, 200, 30),
            "pixel {i} nao ficou solido"
        );
    }
}
