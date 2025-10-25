use image::{ImageBuffer, Rgba};
use sdl2::{event::Event, keyboard::Keycode, pixels::PixelFormatEnum};
use std::{fs::File, io::BufRead, io::BufReader, path::Path};

const ANCHO: usize = 800;
const ALTO: usize = 800;
const DEFAULT_OBJ: &str = "assets/spaceship.obj"; // <- tu modelo por defecto

/// Color RGBA 8-bit
#[derive(Copy, Clone)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}
impl Color {
    fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    fn to_u32(self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }
}

/// Framebuffer sencillo
struct Framebuffer {
    ancho: usize,
    alto: usize,
    pixeles: Vec<u32>, // ARGB8888
    color_actual: Color,
}
impl Framebuffer {
    fn nuevo(ancho: usize, alto: usize) -> Self {
        Self {
            ancho,
            alto,
            pixeles: vec![0; ancho * alto],
            color_actual: Color::rgba(255, 255, 0, 255),
        }
    }
    fn limpiar(&mut self, color: Color) {
        let v = color.to_u32();
        self.pixeles.fill(v);
    }
    fn set_color(&mut self, c: Color) {
        self.color_actual = c;
    }
    #[inline]
    fn put_pixel(&mut self, x: i32, y: i32, c: Color) {
        if x < 0 || y < 0 || x >= self.ancho as i32 || y >= self.alto as i32 {
            return;
        }
        let idx = (y as usize) * self.ancho + (x as usize);
        self.pixeles[idx] = c.to_u32();
    }
    fn guardar_png<P: AsRef<Path>>(&self, path: P) -> Result<(), String> {
        let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::new(self.ancho as u32, self.alto as u32);
        for y in 0..self.alto {
            for x in 0..self.ancho {
                let p = self.pixeles[y * self.ancho + x];
                let a = ((p >> 24) & 0xFF) as u8;
                let r = ((p >> 16) & 0xFF) as u8;
                let g = ((p >> 8) & 0xFF) as u8;
                let b = (p & 0xFF) as u8;
                img.put_pixel(x as u32, y as u32, Rgba([r, g, b, a]));
            }
        }
        img.save(path).map_err(|e| e.to_string())
    }
}

/// Vecs
#[derive(Copy, Clone, Debug)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}
#[derive(Copy, Clone, Debug)]
struct Vec2i {
    x: i32,
    y: i32,
}

/// Cara triangular con índices a vértices (0-based)
#[derive(Copy, Clone, Debug)]
struct Cara {
    i0: usize,
    i1: usize,
    i2: usize,
}

/// Cargador básico de OBJ (soporta v / f con v, v/vt, v//vn, v/vt/vn; triangula n-gons)
fn cargar_obj<P: AsRef<Path>>(path: P) -> Result<(Vec<Vec3>, Vec<Cara>), String> {
    let f = File::open(&path).map_err(|e| format!("No se pudo abrir OBJ: {e}"))?;
    let br = BufReader::new(f);

    let mut vertices: Vec<Vec3> = Vec::new();
    let mut caras: Vec<Cara> = Vec::new();

    for line in br.lines() {
        let l = line.map_err(|e| e.to_string())?;
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if l.starts_with("v ") {
            let mut it = l.split_whitespace();
            it.next(); // "v"
            let x: f32 = it.next().ok_or("v mal formado")?.parse().map_err(|_| "v.x inválido")?;
            let y: f32 = it.next().ok_or("v mal formado")?.parse().map_err(|_| "v.y inválido")?;
            let z: f32 = it.next().ok_or("v mal formado")?.parse().map_err(|_| "v.z inválido")?;
            vertices.push(Vec3 { x, y, z });
        } else if l.starts_with('f') {
            let mut it = l.split_whitespace();
            it.next(); // "f"
            let idxs: Vec<usize> = it
                .map(|tok| {
                    let v_str = tok.split('/').next().unwrap_or(tok);
                    v_str
                        .parse::<isize>()
                        .map(|k| {
                            if k > 0 {
                                (k - 1) as usize
                            } else {
                                (vertices.len() as isize + k) as usize
                            }
                        })
                        .map_err(|_| format!("Índice de cara inválido: {tok}"))
                })
                .collect::<Result<_, _>>()?;

            if idxs.len() < 3 {
                return Err("Cara con menos de 3 vértices".into());
            }
            for t in 1..(idxs.len() - 1) {
                caras.push(Cara {
                    i0: idxs[0],
                    i1: idxs[t],
                    i2: idxs[t + 1],
                });
            }
        }
    }

    if vertices.is_empty() || caras.is_empty() {
        return Err("OBJ sin vértices o caras".into());
    }
    Ok((vertices, caras))
}

/// Centra, escala y proyecta ortográficamente a pantalla
/// Usa rotación variable para ver el modelo en 3D
fn transformar_a_pantalla(verts: &[Vec3], ancho: i32, alto: i32, rotacion_y: f32, rotacion_x: f32, rotacion_z: f32) -> Vec<Vec2i> {
    // Centroide
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut cz = 0.0f32;
    for v in verts {
        cx += v.x;
        cy += v.y;
        cz += v.z;
    }
    let n = verts.len() as f32;
    cx /= n;
    cy /= n;
    cz /= n;

    // AABB para escalar
    let (mut minx, mut miny, mut minz) = (f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let (mut maxx, mut maxy, mut maxz) = (f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for v in verts {
        minx = minx.min(v.x);
        miny = miny.min(v.y);
        minz = minz.min(v.z);
        maxx = maxx.max(v.x);
        maxy = maxy.max(v.y);
        maxz = maxz.max(v.z);
    }
    let dx = maxx - minx;
    let dy = maxy - miny;
    let dz = maxz - minz;
    let diag = dx.max(dy).max(dz).max(1e-6);

    let menor = (ancho.min(alto)) as f32;
    let escala = 0.7 * menor / diag;

    let ox = (ancho as f32) * 0.5;
    let oy = (alto as f32) * 0.5;

    let mut out: Vec<Vec2i> = Vec::with_capacity(verts.len());
    for v in verts {
        // Centrar
        let x = v.x - cx;
        let y = v.y - cy;
        let z = v.z - cz;
        
        // Rotación Y (horizontal - izquierda/derecha)
        let cos_y = rotacion_y.cos();
        let sin_y = rotacion_y.sin();
        let x1 = x * cos_y + z * sin_y;
        let z1 = -x * sin_y + z * cos_y;
        
        // Rotación X (vertical - arriba/abajo)
        let cos_x = rotacion_x.cos();
        let sin_x = rotacion_x.sin();
        let y2 = y * cos_x - z1 * sin_x;
        let z2 = y * sin_x + z1 * cos_x;
        
        // Rotación Z (plano - rotación en 2D)
        let cos_z = rotacion_z.cos();
        let sin_z = rotacion_z.sin();
        let x3 = x1 * cos_z - y2 * sin_z;
        let y3 = x1 * sin_z + y2 * cos_z;
        
        // Proyectar a 2D (ignorar z para ortográfica)
        let screen_x = x3 * escala + ox;
        let screen_y = y3 * escala + oy;
        
        out.push(Vec2i {
            x: screen_x.round() as i32,
            y: (alto as f32 - screen_y).round() as i32,
        });
    }
    out
}

/// Área doble del triángulo (signada)
#[inline]
fn edge_fn(a: &Vec2i, b: &Vec2i, c: &Vec2i) -> i32 {
    (b.x - a.x) as i32 * (c.y - a.y) as i32 - (b.y - a.y) as i32 * (c.x - a.x) as i32
}

/// Dibuja una línea entre dos puntos usando algoritmo de Bresenham
fn dibujar_linea(frame: &mut Framebuffer, p0: Vec2i, p1: Vec2i, color: Color) {
    let mut x0 = p0.x;
    let mut y0 = p0.y;
    let x1 = p1.x;
    let y1 = p1.y;

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        frame.put_pixel(x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

/// Rasterización por baricentros (sin z-buffer)
fn dibujar_triangulo(frame: &mut Framebuffer, a: Vec2i, b: Vec2i, c: Vec2i, color: Color) {
    let minx = a.x.min(b.x).min(c.x).max(0);
    let miny = a.y.min(b.y).min(c.y).max(0);
    let maxx = a.x.max(b.x).max(c.x).min(frame.ancho as i32 - 1);
    let maxy = a.y.max(b.y).max(c.y).min(frame.alto as i32 - 1);

    let area = edge_fn(&a, &b, &c);
    if area == 0 {
        return;
    }

    for y in miny..=maxy {
        for x in minx..=maxx {
            let p = Vec2i { x, y };
            let w0 = edge_fn(&b, &c, &p);
            let w1 = edge_fn(&c, &a, &p);
            let w2 = edge_fn(&a, &b, &p);
            if (w0 >= 0 && w1 >= 0 && w2 >= 0 && area > 0)
                || (w0 <= 0 && w1 <= 0 && w2 <= 0 && area < 0)
            {
                frame.put_pixel(x, y, color);
            }
        }
    }
}

/// Dibuja el wireframe (bordes) de un triángulo
fn dibujar_wireframe(frame: &mut Framebuffer, a: Vec2i, b: Vec2i, c: Vec2i, color: Color) {
    dibujar_linea(frame, a, b, color);
    dibujar_linea(frame, b, c, color);
    dibujar_linea(frame, c, a, color);
}

/// Dibuja todas las caras recorriendo los índices manualmente
fn render_modelo(
    fb: &mut Framebuffer,
    verts2d: &[Vec2i],
    caras: &[Cara],
    color: Color,
    color_wireframe: Color,
    culling: bool,
    mostrar_wireframe: bool,
) {
    fb.set_color(color);
    for cara in caras {
        let a = verts2d[cara.i0];
        let b = verts2d[cara.i1];
        let c = verts2d[cara.i2];
        if culling {
            let area = edge_fn(&a, &b, &c);
            if area <= 0 {
                continue;
            }
        }
        // Rellenar el triángulo
        dibujar_triangulo(fb, a, b, c, fb.color_actual);
        
        // Dibujar los bordes para ver la estructura
        if mostrar_wireframe {
            dibujar_wireframe(fb, a, b, c, color_wireframe);
        }
    }
}

fn main() -> Result<(), String> {
    // Ruta al .obj: argumento 1 o valor por defecto "assets/spaceship.obj"
    let args: Vec<String> = std::env::args().collect();
    let ruta_obj = if args.len() >= 2 {
        args[1].clone()
    } else {
        DEFAULT_OBJ.to_string()
    };

    if !Path::new(&ruta_obj).exists() {
        return Err(format!(
            "No se encontró el OBJ en '{}'. Asegúrate de tener:\n\
             - El archivo en {}\n\
             - O ejecutar: cargo run --release -- assets/spaceship.obj",
            ruta_obj, DEFAULT_OBJ
        ));
    }

    // Cargar modelo
    let (vertices, caras) = cargar_obj(&ruta_obj)?;
    println!(
        "Cargado OBJ: {} vértices, {} triángulos",
        vertices.len(),
        caras.len()
    );

    // Variables de rotación
    let mut rotacion_x = 20.0_f32.to_radians(); // Rotación inicial en X
    let mut rotacion_y = 30.0_f32.to_radians(); // Rotación inicial en Y
    let mut rotacion_z = 0.0_f32;                // Rotación inicial en Z
    let velocidad_rotacion = 5.0_f32.to_radians(); // 5 grados por tecla presionada
    let mut auto_rotacion = false;               // Rotación automática

    // SDL2
    let sdl = sdl2::init().map_err(|e| e.to_string())?;
    let video = sdl.video().map_err(|e| e.to_string())?;
    let ventana = video
        .window("Software Renderer (Rust + SDL2)", ANCHO as u32, ALTO as u32)
        .position_centered()
        .opengl()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = ventana
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .map_err(|e| e.to_string())?;

    let creator = canvas.texture_creator();
    let mut textura = creator
        .create_texture_streaming(PixelFormatEnum::ARGB8888, ANCHO as u32, ALTO as u32)
        .map_err(|e| e.to_string())?;

    let mut fb = Framebuffer::nuevo(ANCHO, ALTO);
    let color_modelo = Color::rgba(255, 255, 0, 255);          // Amarillo para relleno
    let color_wireframe = Color::rgba(40, 40, 40, 255);       // Gris oscuro para bordes
    let color_fondo = Color::rgba(10, 10, 40, 255);           // Azul oscuro espacial

    // Renderizar una vez y guardar captura automáticamente
    let verts2d_inicial = transformar_a_pantalla(&vertices, ANCHO as i32, ALTO as i32, rotacion_y, rotacion_x, rotacion_z);
    fb.limpiar(color_fondo);
    render_modelo(&mut fb, &verts2d_inicial, &caras, color_modelo, color_wireframe, false, true);
    fb.guardar_png("spaceship_render.png")?;
    println!("✓ Captura inicial guardada en spaceship_render.png");
    println!("\n=== CONTROLES ===");
    println!("Flechas Izquierda/Derecha: Rotar en eje Y (horizontal)");
    println!("Flechas Arriba/Abajo: Rotar en eje X (vertical)");
    println!("Q/E: Rotar en eje Z (plano)");
    println!("ESPACIO: Activar/Desactivar rotación automática");
    println!("R: Resetear rotación a posición inicial");
    println!("S: Guardar captura de pantalla");
    println!("ESC: Salir\n");

    let mut eventos = sdl.event_pump().map_err(|e| e.to_string())?;
    'mainloop: loop {
        for ev in eventos.poll_iter() {
            match ev {
                Event::Quit { .. } => break 'mainloop,
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'mainloop,
                Event::KeyDown {
                    keycode: Some(Keycode::S),
                    ..
                } => {
                    fb.guardar_png("captura.png")?;
                    println!("Captura guardada en captura.png");
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Left),
                    ..
                } => {
                    rotacion_y -= velocidad_rotacion;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Right),
                    ..
                } => {
                    rotacion_y += velocidad_rotacion;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Up),
                    ..
                } => {
                    rotacion_x -= velocidad_rotacion;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Down),
                    ..
                } => {
                    rotacion_x += velocidad_rotacion;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Q),
                    ..
                } => {
                    rotacion_z -= velocidad_rotacion;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::E),
                    ..
                } => {
                    rotacion_z += velocidad_rotacion;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Space),
                    ..
                } => {
                    auto_rotacion = !auto_rotacion;
                    println!("Rotación automática: {}", if auto_rotacion { "ON" } else { "OFF" });
                }
                Event::KeyDown {
                    keycode: Some(Keycode::R),
                    ..
                } => {
                    rotacion_x = 20.0_f32.to_radians();
                    rotacion_y = 30.0_f32.to_radians();
                    rotacion_z = 0.0_f32;
                    println!("Rotación reseteada a posición inicial");
                }
                _ => {}
            }
        }

        // Rotación automática
        if auto_rotacion {
            rotacion_y += velocidad_rotacion * 0.3;
        }

        // Transformar vértices con la rotación actual
        let verts2d = transformar_a_pantalla(&vertices, ANCHO as i32, ALTO as i32, rotacion_y, rotacion_x, rotacion_z);

        fb.limpiar(color_fondo);
        render_modelo(&mut fb, &verts2d, &caras, color_modelo, color_wireframe, false, true);

        textura.with_lock(None, |buf: &mut [u8], pitch: usize| {
            for y in 0..ALTO {
                let fila = &fb.pixeles[y * ANCHO..(y + 1) * ANCHO];
                let dst = &mut buf[y * pitch..y * pitch + ANCHO * 4];
                for (x, p) in fila.iter().enumerate() {
                    let a = ((p >> 24) & 0xFF) as u8;
                    let r = ((p >> 16) & 0xFF) as u8;
                    let g = ((p >> 8) & 0xFF) as u8;
                    let b = (p & 0xFF) as u8;
                    dst[x * 4 + 0] = a; // ARGB8888
                    dst[x * 4 + 1] = r;
                    dst[x * 4 + 2] = g;
                    dst[x * 4 + 3] = b;
                }
            }
        })?;

        canvas.clear();
        canvas.copy(&textura, None, None)?;
        canvas.present();
    }
    Ok(())
}
