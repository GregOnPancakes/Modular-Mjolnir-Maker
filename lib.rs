use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use thiserror::Error;
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const HEAD: &str = "01_Mjolnir_Hammer_Head_System_Clean_Circles.3mf";
const POMMEL: &str = "02_Mjolnir_Classic_Comic_Pommel_Side_Slots.3mf";
const FLAT_HANDLES: &str = "03_Mjolnir_Real_Leather_Handle_Core_Library.3mf";
const DETAILED_HANDLES: &str = "04_Mjolnir_Detailed_Printed_Leather_Handle_Library.3mf";
const STRAPS: &str = "05_Mjolnir_Wrist_Strap_Options.3mf";

const HEAD_BYTES: &[u8] = include_bytes!("../../assets/01_Mjolnir_Hammer_Head_System_Clean_Circles.3mf");
const POMMEL_BYTES: &[u8] = include_bytes!("../../assets/02_Mjolnir_Classic_Comic_Pommel_Side_Slots.3mf");
const FLAT_HANDLE_BYTES: &[u8] = include_bytes!("../../assets/03_Mjolnir_Real_Leather_Handle_Core_Library.3mf");
const DETAILED_HANDLE_BYTES: &[u8] = include_bytes!("../../assets/04_Mjolnir_Detailed_Printed_Leather_Handle_Library.3mf");
const STRAP_BYTES: &[u8] = include_bytes!("../../assets/05_Mjolnir_Wrist_Strap_Options.3mf");

type CachedLibrary = std::result::Result<Vec<Mesh>, String>;
static HEAD_CACHE: OnceLock<CachedLibrary> = OnceLock::new();
static POMMEL_CACHE: OnceLock<CachedLibrary> = OnceLock::new();
static FLAT_HANDLE_CACHE: OnceLock<CachedLibrary> = OnceLock::new();
static DETAILED_HANDLE_CACHE: OnceLock<CachedLibrary> = OnceLock::new();
static STRAP_CACHE: OnceLock<CachedLibrary> = OnceLock::new();

#[derive(Debug, Error)]
enum BuilderError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
}

type Result<T> = std::result::Result<T, BuilderError>;

#[derive(Debug, Clone)]
struct Mesh {
    id: u32,
    name: String,
    vertices: Vec<[f32; 3]>,
    triangles: Vec<[u32; 3]>,
}

#[derive(Debug, Clone, Deserialize)]
struct Combination {
    handle_style: String,
    length: u32,
    thickness: u32,
    lower_end: String,
    strap: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Colours {
    silver: String,
    leather: String,
    strap: String,
}

#[derive(Debug, Serialize)]
struct PreviewPart {
    name: String,
    material: String,
    vertices: Vec<f32>,
    indices: Vec<u32>,
}

#[derive(Debug, Serialize)]
struct ExportResult {
    objects: usize,
    plates: usize,
    path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandleStyle {
    Flat,
    Simple,
    Detailed,
}

fn handle_style(value: &str) -> HandleStyle {
    match value {
        "flat" | "real" => HandleStyle::Flat,
        "simple" => HandleStyle::Simple,
        _ => HandleStyle::Detailed,
    }
}

fn attr_value(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}

fn load_3mf_bytes(bytes: &'static [u8], label: &str) -> Result<Vec<Mesh>> {
    let cursor = Cursor::new(bytes);
    let mut zip = ZipArchive::new(cursor)?;
    let mut xml = String::new();
    zip.by_name("3D/3dmodel.model")?
        .read_to_string(&mut xml)?;

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    let mut meshes = Vec::new();
    let mut current_id = 0;
    let mut current_name = String::new();
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut in_mesh = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.local_name();
                match name.as_ref() {
                    b"object" => {
                        current_id = attr_value(&e, b"id")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                        current_name = attr_value(&e, b"name")
                            .unwrap_or_else(|| format!("Object {current_id}"));
                        vertices.clear();
                        triangles.clear();
                    }
                    b"mesh" => in_mesh = true,
                    b"vertex" if in_mesh => {
                        let x = attr_value(&e, b"x")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0.0);
                        let y = attr_value(&e, b"y")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0.0);
                        let z = attr_value(&e, b"z")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0.0);
                        vertices.push([x, y, z]);
                    }
                    b"triangle" if in_mesh => {
                        let a = attr_value(&e, b"v1")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                        let b = attr_value(&e, b"v2")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                        let c = attr_value(&e, b"v3")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                        triangles.push([a, b, c]);
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"mesh" => in_mesh = false,
                b"object" if !vertices.is_empty() && !triangles.is_empty() => {
                    meshes.push(Mesh {
                        id: current_id,
                        name: current_name.clone(),
                        vertices: vertices.clone(),
                        triangles: triangles.clone(),
                    });
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(BuilderError::Message(format!(
                    "Invalid 3MF XML in {label}: {e}"
                )))
            }
            _ => {}
        }
    }

    if meshes.is_empty() {
        return Err(BuilderError::Message(format!(
            "No mesh objects were found in embedded asset {label}."
        )));
    }

    Ok(meshes)
}

fn cached_library(
    cache: &'static OnceLock<CachedLibrary>,
    bytes: &'static [u8],
    label: &str,
) -> Result<&'static [Mesh]> {
    let result = cache.get_or_init(|| load_3mf_bytes(bytes, label).map_err(|e| e.to_string()));
    match result {
        Ok(meshes) => Ok(meshes.as_slice()),
        Err(message) => Err(BuilderError::Message(message.clone())),
    }
}

fn library(label: &str) -> Result<&'static [Mesh]> {
    match label {
        HEAD => cached_library(&HEAD_CACHE, HEAD_BYTES, HEAD),
        POMMEL => cached_library(&POMMEL_CACHE, POMMEL_BYTES, POMMEL),
        FLAT_HANDLES => cached_library(&FLAT_HANDLE_CACHE, FLAT_HANDLE_BYTES, FLAT_HANDLES),
        DETAILED_HANDLES => cached_library(
            &DETAILED_HANDLE_CACHE,
            DETAILED_HANDLE_BYTES,
            DETAILED_HANDLES,
        ),
        STRAPS => cached_library(&STRAP_CACHE, STRAP_BYTES, STRAPS),
        _ => Err(BuilderError::Message(format!(
            "Unknown embedded asset: {label}"
        ))),
    }
}

fn mesh_by_id(meshes: &[Mesh], id: u32) -> Result<Mesh> {
    meshes
        .iter()
        .find(|mesh| mesh.id == id)
        .cloned()
        .ok_or_else(|| BuilderError::Message(format!("Object {id} is missing.")))
}

fn find_handle(meshes: &[Mesh], combination: &Combination) -> Result<Mesh> {
    let style = handle_style(&combination.handle_style);
    let length_text = format!("{}mm", combination.length);
    let thickness_text = if style == HandleStyle::Flat {
        format!("{}mm Core", combination.thickness)
    } else {
        format!("{}mm Max", combination.thickness)
    };
    let no_pommel = combination.lower_end == "no_pommel";

    meshes
        .iter()
        .find(|mesh| {
            mesh.name.contains(&length_text)
                && mesh.name.contains(&thickness_text)
                && ((no_pommel && mesh.name.contains("No Pommel"))
                    || (!no_pommel && !mesh.name.contains("No Pommel")))
        })
        .cloned()
        .ok_or_else(|| {
            BuilderError::Message(format!(
                "No handle matched {} / {} / {}.",
                combination.handle_style, length_text, thickness_text
            ))
        })
}

fn simplify_leather_relief(mesh: &mut Mesh, max_diameter: u32, visible_length: u32) {
    let min_x = mesh
        .vertices
        .iter()
        .map(|p| p[0])
        .fold(f32::MAX, f32::min);
    let max_x = mesh
        .vertices
        .iter()
        .map(|p| p[0])
        .fold(f32::MIN, f32::max);
    let min_y = mesh
        .vertices
        .iter()
        .map(|p| p[1])
        .fold(f32::MAX, f32::min);
    let max_y = mesh
        .vertices
        .iter()
        .map(|p| p[1])
        .fold(f32::MIN, f32::max);

    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    let detailed_base_radius = max_diameter as f32 * 0.5 - 0.75;
    let simple_base_radius = max_diameter as f32 * 0.5 - 0.25;
    let relief_scale = 1.0 / 3.0;

    for point in &mut mesh.vertices {
        if point[2] > visible_length as f32 + 0.5 {
            continue;
        }

        let dx = point[0] - center_x;
        let dy = point[1] - center_y;
        let radius = (dx * dx + dy * dy).sqrt();

        // Only soften the outside leather surface. Deep slots, sockets and
        // connector geometry remain untouched.
        if radius <= detailed_base_radius - 1.2 || radius <= f32::EPSILON {
            continue;
        }

        let softened_radius =
            simple_base_radius + (radius - detailed_base_radius) * relief_scale;
        let scale = softened_radius / radius;
        point[0] = center_x + dx * scale;
        point[1] = center_y + dy * scale;
    }

    mesh.name = mesh
        .name
        .replace("Printed Leather Handle", "Simple Leather Handle");
}

fn selected(combination: &Combination) -> Result<Vec<(String, String, Mesh)>> {
    let head = library(HEAD)?;
    let style = handle_style(&combination.handle_style);
    let handle_library = if style == HandleStyle::Flat {
        library(FLAT_HANDLES)?
    } else {
        library(DETAILED_HANDLES)?
    };

    let mut handle = find_handle(handle_library, combination)?;
    if style == HandleStyle::Simple {
        simplify_leather_relief(&mut handle, combination.thickness, combination.length);
    }

    let mut parts = vec![
        (
            "head_shell".into(),
            "silver".into(),
            mesh_by_id(head, 1)?,
        ),
        (
            "head_bottom".into(),
            "silver".into(),
            mesh_by_id(head, 2)?,
        ),
        ("handle".into(), "leather".into(), handle),
    ];

    if combination.lower_end == "pommel" {
        parts.push((
            "pommel".into(),
            "silver".into(),
            mesh_by_id(library(POMMEL)?, 1)?,
        ));
    }

    if matches!(
        combination.strap.as_str(),
        "plain_tpu" | "detailed_tpu" | "real"
    ) {
        let id = if combination.strap == "detailed_tpu" {
            2
        } else {
            1
        };
        parts.push((
            "strap".into(),
            "strap".into(),
            mesh_by_id(library(STRAPS)?, id)?,
        ));
    }

    Ok(parts)
}

fn centered(mut vertices: Vec<[f32; 3]>) -> Vec<[f32; 3]> {
    let mut low = [f32::MAX; 3];
    let mut high = [f32::MIN; 3];

    for point in &vertices {
        for axis in 0..3 {
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
    }

    let center_x = (low[0] + high[0]) * 0.5;
    let center_y = (low[1] + high[1]) * 0.5;
    for point in &mut vertices {
        point[0] -= center_x;
        point[1] -= center_y;
    }
    vertices
}

fn assembled(mesh: &Mesh, role: &str, combination: &Combination) -> Vec<[f32; 3]> {
    let mut vertices = centered(mesh.vertices.clone());

    match role {
        "handle" => {
            for point in &mut vertices {
                point[2] -= combination.length as f32;
            }
        }
        "pommel" => {
            let top = mesh
                .vertices
                .iter()
                .map(|p| p[2])
                .fold(f32::MIN, f32::max);
            for point in &mut vertices {
                point[2] -= top + combination.length as f32;
            }
        }
        "strap" => {
            let source = mesh.vertices.clone();
            let min_x = source.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
            let max_x = source.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
            let max_y = source.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
            let mean_z = source.iter().map(|p| p[2]).sum::<f32>() / source.len() as f32;
            let attachment = -(combination.length as f32)
                - if combination.lower_end == "pommel" {
                    22.0
                } else {
                    0.0
                };

            vertices = source
                .into_iter()
                .map(|p| {
                    [
                        (p[0] - (min_x + max_x) * 0.5) * 0.75,
                        (p[2] - mean_z) * 0.75,
                        (p[1] - max_y) * 0.75 + attachment,
                    ]
                })
                .collect();
        }
        _ => {}
    }

    vertices
}

#[tauri::command]
fn preview_combination(
    combo: Combination,
) -> std::result::Result<Vec<PreviewPart>, String> {
    let parts = selected(&combo).map_err(|e| e.to_string())?;
    let mut preview = Vec::new();

    for (role, material, mesh) in parts {
        let assembled_vertices = assembled(&mesh, &role, &combo);
        let max_faces = if role == "head_shell" || role == "handle" {
            12_000
        } else {
            6_000
        };
        let step = ((mesh.triangles.len() + max_faces - 1) / max_faces).max(1);
        let mut vertex_map: HashMap<u32, u32> = HashMap::new();
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for triangle in mesh.triangles.iter().step_by(step) {
            for old_index in triangle {
                let new_index = *vertex_map.entry(*old_index).or_insert_with(|| {
                    let point = assembled_vertices[*old_index as usize];
                    let index = (vertices.len() / 3) as u32;
                    vertices.extend_from_slice(&point);
                    index
                });
                indices.push(new_index);
            }
        }

        preview.push(PreviewPart {
            name: mesh.name,
            material,
            vertices,
            indices,
        });
    }

    Ok(preview)
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn format_float(value: f32) -> String {
    let formatted = format!("{value:.6}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn placeholder_png(_hex: &str) -> Vec<u8> {
    STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .unwrap_or_default()
}

#[tauri::command]
fn export_combination(
    path: String,
    combo: Combination,
    colours: Colours,
) -> std::result::Result<ExportResult, String> {
    export_impl(Path::new(&path), &combo, &colours).map_err(|e| e.to_string())
}

fn export_impl(
    path: &Path,
    combination: &Combination,
    colours: &Colours,
) -> Result<ExportResult> {
    let mut parts = selected(combination)?;
    if combination.strap == "real" {
        parts.retain(|(role, _, _)| role.as_str() != "strap");
    }

    let mut materials = vec!["silver", "leather"];
    if parts
        .iter()
        .any(|(_, material, _)| material.as_str() == "strap")
    {
        materials.push("strap");
    }

    let colour = |material: &str| match material {
        "silver" => colours.silver.as_str(),
        "leather" => colours.leather.as_str(),
        _ => colours.strap.as_str(),
    };

    let mut model = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><model unit=\"millimeter\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" xmlns:p=\"http://schemas.microsoft.com/3dmanufacturing/production/2015/06\"><metadata name=\"Application\">BambuStudio</metadata><metadata name=\"BambuStudio:3mfVersion\">1</metadata><resources><basematerials id=\"100\">",
    );

    for material in &materials {
        model.push_str(&format!(
            "<base name=\"{}\" displaycolor=\"{}\"/>",
            escape_xml(*material),
            colour(*material).to_uppercase()
        ));
    }
    model.push_str("</basematerials>");

    for (index, (_, material, mesh)) in parts.iter().enumerate() {
        model.push_str(&format!(
            "<object id=\"{}\" name=\"{}\" type=\"model\" pid=\"100\" pindex=\"{}\" p:UUID=\"{}\"><mesh><vertices>",
            index + 1,
            escape_xml(&mesh.name),
            materials
                .iter()
                .position(|candidate| *candidate == material.as_str())
                .unwrap_or(0),
            Uuid::new_v4()
        ));

        for point in &mesh.vertices {
            model.push_str(&format!(
                "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>",
                format_float(point[0]),
                format_float(point[1]),
                format_float(point[2])
            ));
        }

        model.push_str("</vertices><triangles>");
        for triangle in &mesh.triangles {
            model.push_str(&format!(
                "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>",
                triangle[0], triangle[1], triangle[2]
            ));
        }
        model.push_str("</triangles></mesh></object>");
    }

    model.push_str("</resources><build>");
    for index in 0..parts.len() {
        model.push_str(&format!("<item objectid=\"{}\"/>", index + 1));
    }
    model.push_str("</build></model>");

    let mut model_settings = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><config>",
    );

    for (index, (_, material, mesh)) in parts.iter().enumerate() {
        let extruder = materials
            .iter()
            .position(|candidate| *candidate == material.as_str())
            .unwrap()
            + 1;
        model_settings.push_str(&format!(
            "<object id=\"{}\"><metadata key=\"name\" value=\"{}\"/><metadata key=\"extruder\" value=\"{}\"/><part id=\"{}\" subtype=\"normal_part\"><metadata key=\"name\" value=\"{}\"/><metadata key=\"matrix\" value=\"1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1\"/><mesh_stat face_count=\"{}\" edges_fixed=\"0\" degenerate_facets=\"0\" facets_removed=\"0\" facets_reversed=\"0\" backwards_edges=\"0\"/></part></object>",
            index + 1,
            escape_xml(&mesh.name),
            extruder,
            index + 1,
            escape_xml(&mesh.name),
            mesh.triangles.len()
        ));
    }

    for (index, (role, material, _)) in parts.iter().enumerate() {
        model_settings.push_str(&format!(
            "<plate><metadata key=\"plater_id\" value=\"{}\"/><metadata key=\"plater_name\" value=\"{:02} — {} — {}\"/><metadata key=\"locked\" value=\"false\"/><metadata key=\"thumbnail_file\" value=\"Metadata/plate_{}.png\"/><model_instance><metadata key=\"object_id\" value=\"{}\"/><metadata key=\"instance_id\" value=\"0\"/><metadata key=\"identify_id\" value=\"{}\"/></model_instance></plate>",
            index + 1,
            index + 1,
            escape_xml(material),
            escape_xml(role),
            index + 1,
            index + 1,
            101 + index
        ));
    }
    model_settings.push_str("</config>");

    let filament_types: Vec<&str> = materials
        .iter()
        .map(|material| if *material == "strap" { "TPU" } else { "PLA" })
        .collect();
    let filament_colours: Vec<String> = materials
        .iter()
        .map(|material| colour(*material).to_uppercase())
        .collect();

    let project_settings = serde_json::json!({
        "printer_model": "Bambu Lab P2S",
        "printer_variant": "0.4",
        "nozzle_diameter": ["0.4"],
        "filament_type": filament_types,
        "filament_colour": filament_colours,
        "filament_vendor": materials.iter().map(|_| "Generic").collect::<Vec<_>>(),
        "layer_height": "0.20",
        "wall_loops": "4",
        "top_shell_layers": "6",
        "bottom_shell_layers": "6",
        "sparse_infill_density": "18%",
        "sparse_infill_pattern": "gyroid",
        "enable_support": "0",
        "support_on_build_plate_only": "1",
        "elefant_foot_compensation": "0.15",
        "brim_type": "auto_brim",
        "brim_width": "8"
    });

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = File::create(path)?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    archive.start_file("[Content_Types].xml", options)?;
    archive.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/><Default Extension=\"config\" ContentType=\"application/octet-stream\"/><Default Extension=\"png\" ContentType=\"image/png\"/></Types>")?;

    archive.start_file("_rels/.rels", options)?;
    archive.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>")?;

    archive.start_file("3D/3dmodel.model", options)?;
    archive.write_all(model.as_bytes())?;

    archive.start_file("Metadata/model_settings.config", options)?;
    archive.write_all(model_settings.as_bytes())?;

    archive.start_file("Metadata/project_settings.config", options)?;
    archive.write_all(
        serde_json::to_string_pretty(&project_settings)
            .unwrap()
            .as_bytes(),
    )?;

    for (index, (_, material, _)) in parts.iter().enumerate() {
        archive.start_file(format!("Metadata/plate_{}.png", index + 1), options)?;
        archive.write_all(&placeholder_png(colour(material.as_str())))?;
    }

    archive.finish()?;

    Ok(ExportResult {
        objects: parts.len(),
        plates: parts.len(),
        path: path.display().to_string(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            preview_combination,
            export_combination
        ])
        .run(tauri::generate_context!())
        .expect("error while running Mjolnir Builder");
}
