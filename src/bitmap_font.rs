use glow::HasContext;

use crate::font_data;

const GLYPH_WIDTH: u32 = 8;
const GLYPH_HEIGHT: u32 = 16;
const GLYPHS_PER_ROW: u32 = 16;
const FONT_START: u32 = 32;
const FONT_END: u32 = 126;

pub struct BitmapFont {
    texture: glow::Texture,
    program: glow::Program,
    vbo: glow::Buffer,
}

impl BitmapFont {
    pub fn new(gl: &glow::Context) -> Self {
        unsafe {
            let font_data = font_data::FONT_DATA;
            let num_chars = (FONT_END - FONT_START + 1) as usize;
            let tex_width = GLYPHS_PER_ROW * GLYPH_WIDTH;
            let tex_height =
                ((num_chars as u32 + GLYPHS_PER_ROW - 1) / GLYPHS_PER_ROW) * GLYPH_HEIGHT;

            let mut pixels = vec![0u8; (tex_width * tex_height) as usize];
            for (i, &byte) in font_data.iter().enumerate() {
                let char_index = i / 16;
                let row_in_char = i % 16;
                let gx = (char_index as u32 % GLYPHS_PER_ROW) * GLYPH_WIDTH;
                let gy = (char_index as u32 / GLYPHS_PER_ROW) * GLYPH_HEIGHT + row_in_char as u32;
                for bit in 0..8 {
                    if byte & (0x80 >> bit) != 0 {
                        let px = (gx + bit) as usize;
                        let py = gy as usize;
                        pixels[py * tex_width as usize + px] = 255;
                    }
                }
            }

            let texture = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::LUMINANCE as i32,
                tex_width as i32,
                tex_height as i32,
                0,
                glow::LUMINANCE,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&pixels)),
            );

            let vs_src = "\
                #version 100\n\
                precision highp float;\n\
                attribute vec2 a_pos;\n\
                attribute vec2 a_tex;\n\
                uniform vec2 u_resolution;\n\
                varying vec2 v_tex;\n\
                void main() {\n\
                    vec2 clip = (a_pos / u_resolution) * 2.0 - 1.0;\n\
                    gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);\n\
                    v_tex = a_tex;\n\
                }";
            let fs_src = "\
                #version 100\n\
                precision highp float;\n\
                uniform sampler2D u_texture;\n\
                uniform vec4 u_color;\n\
                varying vec2 v_tex;\n\
                void main() {\n\
                    float a = texture2D(u_texture, v_tex).r;\n\
                    gl_FragColor = vec4(u_color.rgb, u_color.a * a);\n\
                }";

            let vs = gl.create_shader(glow::VERTEX_SHADER).unwrap();
            gl.shader_source(vs, vs_src);
            gl.compile_shader(vs);

            let fs = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
            gl.shader_source(fs, fs_src);
            gl.compile_shader(fs);

            let program = gl.create_program().unwrap();
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            gl.delete_shader(vs);
            gl.delete_shader(fs);

            let vbo = gl.create_buffer().unwrap();

            BitmapFont { texture, program, vbo }
        }
    }

    pub fn glyph_width(&self) -> u32 {
        GLYPH_WIDTH
    }

    pub fn glyph_height(&self) -> u32 {
        GLYPH_HEIGHT
    }

    pub fn measure(&self, text: &str) -> (u32, u32) {
        let width = text.chars().count() as u32 * GLYPH_WIDTH;
        (width, GLYPH_HEIGHT)
    }

    pub fn render_text(
        &self,
        gl: &glow::Context,
        text: &str,
        x: f32,
        y: f32,
        color: (f32, f32, f32, f32),
        screen_w: f32,
        screen_h: f32,
    ) {
        unsafe {
            gl.use_program(Some(self.program));
            gl.uniform_2_f32(
                gl.get_uniform_location(self.program, "u_resolution").as_ref(),
                screen_w,
                screen_h,
            );
            gl.uniform_4_f32(
                gl.get_uniform_location(self.program, "u_color").as_ref(),
                color.0, color.1, color.2, color.3,
            );

            let tex_loc = gl.get_uniform_location(self.program, "u_texture");
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.uniform_1_i32(tex_loc.as_ref(), 0);

            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, 8);

            let num_chars = (FONT_END - FONT_START + 1) as f32;
            let tex_w = GLYPHS_PER_ROW as f32;
            let tex_h = (num_chars / tex_w).ceil();

            let mut cx = x;
            for ch in text.chars() {
                let code = ch as u32;
                if code < FONT_START || code > FONT_END {
                    cx += GLYPH_WIDTH as f32;
                    continue;
                }
                let idx = (code - FONT_START) as f32;
                let tx = (idx % tex_w) / tex_w;
                let ty = (idx / tex_w).floor() / tex_h;

                let gw = GLYPH_WIDTH as f32;
                let gh = GLYPH_HEIGHT as f32;
                let du = gw / (tex_w * GLYPH_WIDTH as f32);
                let dv = gh / (tex_h * GLYPH_HEIGHT as f32);

                let verts: [f32; 24] = [
                    cx,      y,       tx,     ty,
                    cx + gw, y,       tx + du, ty,
                    cx + gw, y + gh,  tx + du, ty + dv,
                    cx,      y,       tx,     ty,
                    cx + gw, y + gh,  tx + du, ty + dv,
                    cx,      y + gh,  tx,     ty + dv,
                ];
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    core::slice::from_raw_parts(verts.as_ptr() as *const u8, 24 * 4),
                    glow::STREAM_DRAW,
                );
                gl.draw_arrays(glow::TRIANGLES, 0, 6);

                cx += gw;
            }

            gl.disable_vertex_attrib_array(0);
            gl.disable_vertex_attrib_array(1);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            // Reset blend state to the ruffle renderer's expected default
            // (ONE, ONE_MINUS_SRC_ALPHA). Using glBlendFunc(SRC_ALPHA, ...)
            // for font rendering would leave the wrong source factor,
            // causing gradients and transparency to appear darker.
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.enable(glow::BLEND);
            gl.bind_vertex_array(None);
            gl.use_program(None);
        }
    }
}
