use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use thiserror::Error;
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const HEAD: &str = "01_Mjolnir_Hammer_Head_System_Clean_Circles.3mf";
const POMMEL: &str = "02_Mjolnir_Classic_Comic_Pommel_Side_Slots.3mf";
const REAL: &str = "03_Mjolnir_Real_Leather_Handle_Core_Library.3mf";
const PRINTED: &str = "04_Mjolnir_Detailed_Printed_Leather_Handle_Library.3mf";
const STRAPS: &str = "05_Mjolnir_Wrist_Strap_Options.3mf";

#[derive(Debug, Error)]
enum BuilderError {
    #[error("{0}")] Message(String),
    #[error(transparent)] Io(#[from] std::io::Error),
    #[error(transparent)] Zip(#[from] zip::result::ZipError),
}
type Result<T> = std::result::Result<T, BuilderError>;

#[derive(Debug, Clone)]
struct Mesh { id:u32, name:String, vertices:Vec<[f32;3]>, triangles:Vec<[u32;3]> }

#[derive(Debug, Clone, Deserialize)]
struct Combination { handle_style:String, length:u32, thickness:u32, lower_end:String, strap:String }
#[derive(Debug, Clone, Deserialize)]
struct Colours { silver:String, leather:String, strap:String }
#[derive(Debug, Serialize)]
struct PreviewPart { name:String, material:String, vertices:Vec<f32>, indices:Vec<u32> }
#[derive(Debug, Serialize)]
struct ExportResult { objects:usize, plates:usize, path:String }

fn assets_dir(app:&AppHandle)->Result<PathBuf>{
    let resource=app.path().resource_dir().map_err(|e|BuilderError::Message(e.to_string()))?;
    let candidates=[resource.join("assets"),resource.join("_up_").join("assets"),PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("assets")];
    candidates.into_iter().find(|p|p.join(HEAD).exists()).ok_or_else(||BuilderError::Message("Bundled Mjolnir assets could not be found.".into()))
}

fn attr_value(e:&quick_xml::events::BytesStart<'_>, key:&[u8])->Option<String>{
    e.attributes().flatten().find(|a|a.key.as_ref()==key).and_then(|a|String::from_utf8(a.value.into_owned()).ok())
}

fn load_3mf(path:&Path)->Result<Vec<Mesh>>{
    let f=File::open(path)?; let mut zip=ZipArchive::new(f)?; let mut xml=String::new();
    zip.by_name("3D/3dmodel.model")?.read_to_string(&mut xml)?;
    let mut r=Reader::from_str(&xml); r.config_mut().trim_text(true);
    let mut meshes=Vec::new(); let mut cur_id=0; let mut cur_name=String::new(); let mut verts=Vec::new(); let mut tris=Vec::new(); let mut in_mesh=false;
    loop { match r.read_event(){
        Ok(Event::Start(e))|Ok(Event::Empty(e))=>{ let n=e.local_name(); match n.as_ref(){
            b"object"=>{cur_id=attr_value(&e,b"id").and_then(|v|v.parse().ok()).unwrap_or(0);cur_name=attr_value(&e,b"name").unwrap_or_else(||format!("Object {cur_id}"));verts.clear();tris.clear();},
            b"mesh"=>in_mesh=true,
            b"vertex" if in_mesh=>{let x=attr_value(&e,b"x").and_then(|v|v.parse().ok()).unwrap_or(0.);let y=attr_value(&e,b"y").and_then(|v|v.parse().ok()).unwrap_or(0.);let z=attr_value(&e,b"z").and_then(|v|v.parse().ok()).unwrap_or(0.);verts.push([x,y,z]);},
            b"triangle" if in_mesh=>{let a=attr_value(&e,b"v1").and_then(|v|v.parse().ok()).unwrap_or(0);let b=attr_value(&e,b"v2").and_then(|v|v.parse().ok()).unwrap_or(0);let c=attr_value(&e,b"v3").and_then(|v|v.parse().ok()).unwrap_or(0);tris.push([a,b,c]);}, _=>{} }
        },
        Ok(Event::End(e))=>match e.local_name().as_ref(){b"mesh"=>in_mesh=false,b"object"=>if !verts.is_empty()&&!tris.is_empty(){meshes.push(Mesh{id:cur_id,name:cur_name.clone(),vertices:verts.clone(),triangles:tris.clone()});},_=>{}},
        Ok(Event::Eof)=>break, Err(e)=>return Err(BuilderError::Message(format!("Invalid 3MF XML: {e}"))), _=>{}
    }}
    if meshes.is_empty(){return Err(BuilderError::Message(format!("No mesh objects in {}",path.display())))} Ok(meshes)
}

fn mesh_by_id(v:&[Mesh],id:u32)->Result<Mesh>{v.iter().find(|m|m.id==id).cloned().ok_or_else(||BuilderError::Message(format!("Object {id} missing")))}
fn find_handle(v:&[Mesh],c:&Combination)->Result<Mesh>{
    let len=format!("{}mm",c.length); let thick=if c.handle_style=="printed"{format!("{}mm Max",c.thickness)}else{format!("{}mm Core",c.thickness)}; let no=c.lower_end=="no_pommel";
    v.iter().find(|m|m.name.contains(&len)&&m.name.contains(&thick)&&((no&&m.name.contains("No Pommel"))||(!no&&!m.name.contains("No Pommel")))).cloned().ok_or_else(||BuilderError::Message(format!("Handle not found: {}",len)))
}
fn selected(app:&AppHandle,c:&Combination)->Result<Vec<(String,String,Mesh)>>{
    let d=assets_dir(app)?; let head=load_3mf(&d.join(HEAD))?;let handles=load_3mf(&d.join(if c.handle_style=="printed"{PRINTED}else{REAL}))?;
    let mut out=vec![("head_shell".into(),"silver".into(),mesh_by_id(&head,1)?),("head_bottom".into(),"silver".into(),mesh_by_id(&head,2)?),("handle".into(),"leather".into(),find_handle(&handles,c)?)];
    if c.lower_end=="pommel"{out.push(("pommel".into(),"silver".into(),mesh_by_id(&load_3mf(&d.join(POMMEL))?,1)?));}
    if c.strap=="plain_tpu"||c.strap=="detailed_tpu"||c.strap=="real"{let id=if c.strap=="detailed_tpu"{2}else{1};out.push(("strap".into(),"strap".into(),mesh_by_id(&load_3mf(&d.join(STRAPS))?,id)?));}
    Ok(out)
}
fn centered(mut v:Vec<[f32;3]>)->Vec<[f32;3]>{let (mut lo,mut hi)=([f32::MAX;3],[f32::MIN;3]);for p in &v{for i in 0..3{lo[i]=lo[i].min(p[i]);hi[i]=hi[i].max(p[i]);}}let cx=(lo[0]+hi[0])/2.;let cy=(lo[1]+hi[1])/2.;for p in &mut v{p[0]-=cx;p[1]-=cy;}v}
fn assembled(mesh:&Mesh,role:&str,c:&Combination)->Vec<[f32;3]>{let mut v=centered(mesh.vertices.clone());match role{
    "handle"=>{for p in &mut v{p[2]-=c.length as f32;if c.handle_style=="real"{let s=(c.thickness as f32+5.)/c.thickness as f32;p[0]*=s;p[1]*=s;}}},
    "pommel"=>{let top=mesh.vertices.iter().map(|p|p[2]).fold(f32::MIN,f32::max);for p in &mut v{p[2]-=top+c.length as f32;}},
    "strap"=>{let src=mesh.vertices.clone();let minx=src.iter().map(|p|p[0]).fold(f32::MAX,f32::min);let maxx=src.iter().map(|p|p[0]).fold(f32::MIN,f32::max);let maxy=src.iter().map(|p|p[1]).fold(f32::MIN,f32::max);let meanz=src.iter().map(|p|p[2]).sum::<f32>()/src.len() as f32;let attach=-(c.length as f32)-if c.lower_end=="pommel"{22.}else{0.};v=src.into_iter().map(|p|[(p[0]-(minx+maxx)/2.)*.75,(p[2]-meanz)*.75,(p[1]-maxy)*.75+attach]).collect();},_=>{}}
    v
}

#[tauri::command]
fn preview_combination(app:AppHandle,combo:Combination)->std::result::Result<Vec<PreviewPart>,String>{
    let parts=selected(&app,&combo).map_err(|e|e.to_string())?;let mut out=Vec::new();for(role,material,m)in parts{let av=assembled(&m,&role,&combo);let max_faces=if role=="head_shell"||role=="handle"{12000}else{6000};let step=(m.triangles.len()+max_faces-1)/max_faces;let step=step.max(1);let mut map:HashMap<u32,u32>=HashMap::new();let mut vv=Vec::new();let mut ii=Vec::new();for t in m.triangles.iter().step_by(step){for old in t{let ni=*map.entry(*old).or_insert_with(||{let p=av[*old as usize];let i=(vv.len()/3)as u32;vv.extend_from_slice(&p);i});ii.push(ni);}}out.push(PreviewPart{name:m.name,material,vertices:vv,indices:ii});}Ok(out)
}

fn esc(s:&str)->String{s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;")}
fn fmt(v:f32)->String{let s=format!("{v:.6}");s.trim_end_matches('0').trim_end_matches('.').to_string()}
fn png_solid(hex:&str)->Vec<u8>{
    // 1x1 PNG; colour remains encoded in project metadata. Kept as valid thumbnail placeholder.
    let _=hex; base64::decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=").unwrap_or_default()
}

#[tauri::command]
fn export_combination(app:AppHandle,path:String,combo:Combination,colours:Colours)->std::result::Result<ExportResult,String>{
    export_impl(&app,Path::new(&path),&combo,&colours).map_err(|e|e.to_string())
}
fn export_impl(app:&AppHandle,path:&Path,c:&Combination,colors:&Colours)->Result<ExportResult>{
    let mut parts=selected(app,c)?; if c.strap=="real"{parts.retain(|(r,_,_)|r!="strap");}
    let mut mats=vec!["silver","leather"];if parts.iter().any(|(_,m,_)|m=="strap"){mats.push("strap");}
    let color=|m:&str|match m{"silver"=>colors.silver.as_str(),"leather"=>colors.leather.as_str(),_=>colors.strap.as_str()};
    let mut model=String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><model unit=\"millimeter\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" xmlns:p=\"http://schemas.microsoft.com/3dmanufacturing/production/2015/06\"><metadata name=\"Application\">BambuStudio</metadata><metadata name=\"BambuStudio:3mfVersion\">1</metadata><resources><basematerials id=\"100\">");
    for m in &mats{model.push_str(&format!("<base name=\"{}\" displaycolor=\"{}\"/>",esc(m),color(m).to_uppercase()));}model.push_str("</basematerials>");
    for(i,(_,mat,mesh))in parts.iter().enumerate(){model.push_str(&format!("<object id=\"{}\" name=\"{}\" type=\"model\" pid=\"100\" pindex=\"{}\" p:UUID=\"{}\"><mesh><vertices>",i+1,esc(&mesh.name),mats.iter().position(|x|x==mat).unwrap_or(0),Uuid::new_v4()));for p in &mesh.vertices{model.push_str(&format!("<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>",fmt(p[0]),fmt(p[1]),fmt(p[2])));}model.push_str("</vertices><triangles>");for t in &mesh.triangles{model.push_str(&format!("<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>",t[0],t[1],t[2]));}model.push_str("</triangles></mesh></object>");}model.push_str("</resources><build>");for i in 0..parts.len(){model.push_str(&format!("<item objectid=\"{}\"/>",i+1));}model.push_str("</build></model>");
    let mut cfg=String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><config>");for(i,(_,mat,mesh))in parts.iter().enumerate(){let ex=mats.iter().position(|x|x==mat).unwrap()+1;cfg.push_str(&format!("<object id=\"{}\"><metadata key=\"name\" value=\"{}\"/><metadata key=\"extruder\" value=\"{}\"/><part id=\"{}\" subtype=\"normal_part\"><metadata key=\"name\" value=\"{}\"/><metadata key=\"matrix\" value=\"1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1\"/><mesh_stat face_count=\"{}\" edges_fixed=\"0\" degenerate_facets=\"0\" facets_removed=\"0\" facets_reversed=\"0\" backwards_edges=\"0\"/></part></object>",i+1,esc(&mesh.name),ex,i+1,esc(&mesh.name),mesh.triangles.len()));}
    for(i,(role,mat,_))in parts.iter().enumerate(){cfg.push_str(&format!("<plate><metadata key=\"plater_id\" value=\"{}\"/><metadata key=\"plater_name\" value=\"{:02} — {} — {}\"/><metadata key=\"locked\" value=\"false\"/><metadata key=\"thumbnail_file\" value=\"Metadata/plate_{}.png\"/><model_instance><metadata key=\"object_id\" value=\"{}\"/><metadata key=\"instance_id\" value=\"0\"/><metadata key=\"identify_id\" value=\"{}\"/></model_instance></plate>",i+1,i+1,esc(mat),esc(role),i+1,i+1,101+i));}cfg.push_str("</config>");
    let types:Vec<&str>=mats.iter().map(|m|if *m=="strap"{"TPU"}else{"PLA"}).collect();let cols:Vec<String>=mats.iter().map(|m|color(m).to_uppercase()).collect();let settings=serde_json::json!({"printer_model":"Bambu Lab P2S","printer_variant":"0.4","nozzle_diameter":["0.4"],"filament_type":types,"filament_colour":cols,"filament_vendor":mats.iter().map(|_|"Generic").collect::<Vec<_>>(),"layer_height":"0.20","wall_loops":"4","top_shell_layers":"6","bottom_shell_layers":"6","sparse_infill_density":"18%","sparse_infill_pattern":"gyroid","enable_support":"0","support_on_build_plate_only":"1","elefant_foot_compensation":"0.15","brim_type":"auto_brim","brim_width":"8"});
    if let Some(parent)=path.parent(){std::fs::create_dir_all(parent)?;}let file=File::create(path)?;let mut z=ZipWriter::new(file);let opt=SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    z.start_file("[Content_Types].xml",opt)?;z.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/><Default Extension=\"config\" ContentType=\"application/octet-stream\"/><Default Extension=\"png\" ContentType=\"image/png\"/></Types>")?;
    z.start_file("_rels/.rels",opt)?;z.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>")?;
    z.start_file("3D/3dmodel.model",opt)?;z.write_all(model.as_bytes())?;z.start_file("Metadata/model_settings.config",opt)?;z.write_all(cfg.as_bytes())?;z.start_file("Metadata/project_settings.config",opt)?;z.write_all(serde_json::to_string_pretty(&settings).unwrap().as_bytes())?;for(i,(_,mat,_))in parts.iter().enumerate(){z.start_file(format!("Metadata/plate_{}.png",i+1),opt)?;z.write_all(&png_solid(color(mat)))?;}z.finish()?;
    Ok(ExportResult{objects:parts.len(),plates:parts.len(),path:path.display().to_string()})
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(){tauri::Builder::default().plugin(tauri_plugin_dialog::init()).invoke_handler(tauri::generate_handler![preview_combination,export_combination]).run(tauri::generate_context!()).expect("error while running Mjolnir Builder");}
