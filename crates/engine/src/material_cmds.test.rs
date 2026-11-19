use super::*;
use crate::{MaterialOptions, execute, file_cmds, material_bake};
use craft_texture::{Image, Kind};
use materialcraft_core::{ComponentUsage, GraphInstance, GraphPort, Package, Preset, Resource, ResourceBytes, Value};
use materialcraft_formats::{Depth, encode};

fn node(graph: &mut Graph, def: &str, params: &[(&str, Value)]) -> materialcraft_core::NodeId {
    let id = graph.add_node(def, [0.0; 2]);
    graph.node_mut(id).unwrap().params.extend(params.iter().map(|(k, v)| (k.to_string(), v.clone())));
    id
}

fn output(graph: &mut Graph, source: materialcraft_core::NodeId, port: &str, id: &str, usage: &str) {
    let out = node(graph, "output", &[("identifier", Value::Text(id.into())), ("usage", Value::Enum(ComponentUsage::index(usage)))]);
    graph.connect(source, port, out, "input").unwrap();
}

fn fixture() -> Session {
    let mut project = Package::new("Studio");
    project.graphs.push(Graph::new("Unused"));
    let mut graph = Graph::new("Oak");
    graph.folder = "Materials".into();
    graph.size = [5, 4];
    for (owner, usage, gray) in [("a.sbs", "height", 0.22), ("b.sbs", "metallic", 0.75)] {
        let mut helper = Graph::new("Grain");
        let image = node(&mut helper, "bitmap", &[("resource", Value::Text("grain".into()))]);
        output(&mut helper, image, "output", "grain", "height");
        let mut pkg = Package::new("Helpers");
        pkg.graphs.push(helper);
        let data = encode(&Image::filled(16, 16, Kind::Gray, [gray; 4]), "png", Depth::U16, false).unwrap();
        pkg.resources.push(Resource::new("grain", "image.png", ResourceBytes::new(data)));
        project.dependencies.insert(owner.into(), std::sync::Arc::new(pkg));
        let inst = node(&mut graph, "graph_instance", &[]);
        graph.node_mut(inst).unwrap().instance = Some(GraphInstance {
            package: owner.into(),
            graph: "Grain".into(),
            inputs: vec![],
            outputs: vec![GraphPort { identifier: "grain".into(), type_code: 1 }],
            defaults: Default::default(),
            label: String::new(),
        });
        output(&mut graph, inst, "grain", usage, usage);
    }
    let source = node(&mut graph, "uniform", &[]);
    output(&mut graph, source, "output", "albedo", "baseColor");
    let mesh = node(&mut graph, "input_grayscale", &[("identifier", Value::Text("curvature".into()))]);
    output(&mut graph, mesh, "output", "mesh", "roughness");
    project.graphs.push(graph);
    let mut session = Session::new(project);
    session.graph = 1;
    execute(&mut session, "graph.exposed.add", &json!({"id":"tint","bind":{"node":source,"param":"color"}})).unwrap();
    execute(&mut session, "graph.exposed.set", &json!({"id":"tint","value":[0.4,0.2,0.1,1.0]})).unwrap();
    session.graph_mut().unwrap().presets.push(Preset { name: "Dark".into(), values: [("tint".into(), Value::Color([0.15, 0.1, 0.05, 1.0]))].into() });
    session
}

#[test]
fn published_entry_cooks_independently_with_values_sizes_and_image_inputs() {
    let source = fixture();
    let bytes = material_bytes(&source, source.graph).unwrap();
    let (pkg, entry) = materialcraft_formats::material::read_material(&bytes).unwrap();
    assert_eq!(pkg.graphs.len(), 3);
    assert_eq!(pkg.resources.len(), 2);
    assert!(pkg.dependencies.is_empty());
    assert_eq!(pkg.graph(entry).unwrap().sbs_path(), "Oak");
    assert!(pkg.graphs.iter().all(|g| g.name != "Unused"));
    let mut opened = Session::new(Package::new("other"));
    file_cmds::open_bytes(&mut opened, &bytes, Some("Oak.mcmat".into())).unwrap();
    let info = material_bake::info(&opened, &J::Null).unwrap();
    assert_eq!(info["graph"], "Oak");
    assert_eq!(info["parameters"][0]["default"], json!(source.graph().unwrap().exposed[0].default));
    assert_eq!(info["parameters"][0]["value"], json!(source.graph().unwrap().input_value("tint")));
    assert_eq!(info["presets"][0]["name"], "Dark");
    let mut variations = Vec::new();
    for size in [32, 64] {
        for wear in [0.1, 0.7] {
            for gray in [0.2, 0.8] {
                let mesh = encode(&Image::filled(16, 16, Kind::Gray, [gray; 4]), "png", Depth::U16, false).unwrap();
                let p = json!({"profile":"maps","size":size,"preset":"Dark","values":{"tint":[wear,0.1,0.05,1.0]},"inputs":{"curvature":"mesh.png"}});
                let bake = |s: &Session| {
                    material_bake::export(s, &p, |path| {
                        assert_eq!(path, "mesh.png");
                        Ok(mesh.clone())
                    })
                    .unwrap()
                };
                let (original, _) = bake(&source);
                let (packed, report) = bake(&opened);
                assert_eq!(original.len(), 4);
                assert_eq!(packed.len(), original.len());
                assert_eq!(report["maps"][0]["size"], json!([size, size / 2]));
                for (a, b) in original.iter().zip(&packed) {
                    assert_eq!(a.file, b.file);
                    assert_eq!(a.bytes, b.bytes, "{}", a.file);
                }
                let sample = |name: &str, linear: bool| {
                    let map = packed.iter().find(|f| f.file == name).unwrap();
                    materialcraft_formats::decode(&map.bytes, linear).unwrap().get(0, 0, false)[0]
                };
                assert!((f64::from(sample("albedo.png", false)) - wear).abs() < 0.01);
                assert!((sample("mesh.png", true) - gray).abs() < 0.01);
                variations.push(report["material"]["parameters"][0]["value"].clone());
            }
        }
    }
    assert_ne!(variations[0], variations[2]);
    assert_eq!(info, material_bake::info(&opened, &J::Null).unwrap(), "cook overrides leaked");
    assert!(
        material_bake::cook(&opened, &json!({"values":{"missing":0}}), |path| Err(crate::EngineError::Other(format!("unexpected file read: {path}")))).is_err()
    );
    let error = material_bake::cook(&opened, &json!({"inputs":{"missing":"x"}}), |path| panic!("unexpected file read: {path}")).unwrap_err();
    assert!(error.to_string().contains("unknown image input `missing`"));
}

#[test]
fn rejects_missing_ambiguous_and_uncookable_materials() {
    let mut session = fixture();
    assert!(material_bytes(&session, 0).is_err());
    session.pkg.dependencies.remove("b.sbs");
    assert!(material_bytes(&session, 1).is_err());
    let empty = Package::new("Empty");
    assert!(materialcraft_formats::material::entry_graph(&empty).is_err());
    let mut ambiguous = empty;
    ambiguous.graphs.extend([Graph::new("A"), Graph::new("B")]);
    assert!(materialcraft_formats::material::entry_graph(&ambiguous).is_err());
}

#[test]
fn preset_values_apply_before_explicit_overrides() {
    let s = fixture();
    let options = MaterialOptions::from_params(s.graph().unwrap(), &json!({"preset":"Dark"}), |path| {
        Err(crate::EngineError::Other(format!("unexpected file read: {path}")))
    })
    .unwrap();
    assert_eq!(options.values["tint"], Value::Color([0.15, 0.1, 0.05, 1.0]));
}

#[test]
fn publishing_sbs_and_project_archives_survives_removing_the_entire_source_tree() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path();
    let authored = dir.join("authored");
    std::fs::create_dir_all(&authored).unwrap();
    let source = fixture();
    for filename in ["a.sbs", "b.sbs"] {
        let dependency = &source.pkg.dependencies[filename];
        let files = materialcraft_formats::project::ProjectFiles::build(dependency, filename, false).unwrap();
        materialcraft_formats::project::write_files(&authored.join(filename), &files).unwrap();
    }
    for ext in ["sbs", "mcraft"] {
        let path = authored.join(format!("studio.{ext}"));

        // Save the independent project beside its library packages and their original bitmaps
        let files = materialcraft_formats::project::ProjectFiles::build(&source.pkg, &format!("studio.{ext}"), ext == "mcraft").unwrap();
        if ext == "mcraft" {
            std::fs::write(&path, files.zip().unwrap()).unwrap();
        } else {
            materialcraft_formats::project::write_files(&path, &files).unwrap();
        }
        let mut editor = Session::new(Package::new("Empty"));
        execute(&mut editor, "file.open", &json!({"path":path})).unwrap();
        execute(&mut editor, "file.exportMaterial", &json!({"graph":"Materials/Oak","path":dir.join(format!("{ext}.mcmat"))})).unwrap();
    }
    std::fs::remove_dir_all(&authored).unwrap();
    for ext in ["sbs", "mcraft"] {
        let mut viewer = Session::new(Package::new("Empty"));
        execute(&mut viewer, "file.open", &json!({"path":dir.join(format!("{ext}.mcmat"))})).unwrap();
        let settings = json!({"profile":"maps","preset":"Dark","size":32});
        let bake = |s: &Session| {
            material_bake::export(s, &settings, |path| Err(crate::EngineError::Other(format!("unexpected file read: {path}"))))
                .unwrap()
                .0
                .into_iter()
                .map(|f| (f.file, f.bytes))
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        assert_eq!(bake(&source), bake(&viewer));
    }
}

#[test]
fn snapshots_transitive_function_calls_and_rejects_recursion_or_missing_assets() {
    use materialcraft_core::FunctionGraph;
    let mut s = fixture();
    let call = |owner: &str, name: &str| {
        let mut f = FunctionGraph::default();
        f.output = Some(f.add("instance", &[], None));
        f.nodes[0].type_code = 256;
        f.nodes[0].instance = Some(GraphInstance {
            package: owner.into(),
            graph: name.into(),
            inputs: vec![],
            outputs: vec![],
            defaults: Default::default(),
            label: String::new(),
        });
        f
    };
    let mut utils = Package::new("Functions");
    let mut constant = Graph::new("Amount");
    constant.function_type = 256;
    constant.function = Some(FunctionGraph::constant(Value::Float(0.4)));
    utils.graphs.push(constant);
    let mut wrapper = Graph::new("Wrapper");
    wrapper.function_type = 256;
    wrapper.function = Some(call("", "Amount"));
    utils.graphs.push(wrapper);
    s.pkg.dependencies.insert("math.sbs".into(), std::sync::Arc::new(utils));
    let graph = s.graph_mut().unwrap();
    let fx = graph.nodes.iter_mut().find(|n| n.def == "uniform").unwrap();
    fx.functions.insert("color_mode".into(), call("math.sbs", "Wrapper"));
    let bytes = materialcraft_formats::material::write_material(&s.pkg, s.graph).unwrap();
    let (p, root) = materialcraft_formats::material::read_material(&bytes).unwrap();
    assert_eq!(p.graphs.iter().filter(|g| g.function.is_some()).count(), 2);
    assert_eq!(p.graph(root).unwrap().name, "Oak");
    for reference in p.graphs.iter().flat_map(|g| g.instances()) {
        assert!(reference.package.is_empty() && p.graph_by_name(&reference.graph).is_some());
    }
    let lib = std::sync::Arc::make_mut(s.pkg.dependencies.get_mut("math.sbs").unwrap());
    lib.graphs[0].function = Some(call("", "Wrapper"));
    assert!(materialcraft_formats::material::write_material(&s.pkg, s.graph).unwrap_err().to_string().contains("cyclic"));
    std::sync::Arc::make_mut(s.pkg.dependencies.get_mut("a.sbs").unwrap()).resources.clear();
    assert!(materialcraft_formats::material::write_material(&s.pkg, s.graph).unwrap_err().to_string().contains("missing resource"));
}

fn export_fixture(source: &str, params: &[(&str, Value)], usage: &str) -> Session {
    let mut graph = Graph::new("Surface");
    graph.size = [5, 5];
    let generator = node(&mut graph, source, params);
    output(&mut graph, generator, "output", "result", usage);

    let mut package = Package::new("Studio");
    package.graphs.push(graph);
    Session::new(package)
}

#[test]
fn grayscale_base_color_uses_the_transfer_declared_to_materialx() {
    let session = export_fixture("uniform", &[("color", Value::Color([0.5; 4])), ("color_mode", Value::Bool(false))], "baseColor");

    let (files, report) = material_bake::export(&session, &json!({"profile":"materialx"}), material_bake::read_input).unwrap();
    let map = &report["maps"][0];
    let image = image::load_from_memory(&files.iter().find(|file| file.file == map["file"]).unwrap().bytes).unwrap();

    assert_eq!(map["colorSpace"], "srgb");
    assert_eq!(image.into_rgb8().get_pixel(0, 0).0, [188; 3]);
}

#[test]
fn an_overridden_color_input_keeps_the_published_transfer() {
    let session =
        export_fixture("input_color", &[("identifier", Value::Text("swatch".into())), ("usage", Value::Enum(ComponentUsage::index("baseColor")))], "baseColor");
    let png = encode(&Image::filled(32, 32, Kind::Color, [0.21586; 4]), "png", Depth::U8, true).unwrap();

    let cooked = material_bake::cook(&session, &json!({"inputs":{"swatch":"color.png"}}), |_| Ok(png.clone())).unwrap();

    assert!((cooked["outputs"][0]["mean"][0].as_f64().unwrap() - 0.21586).abs() < 0.001);
}

#[test]
fn normal_node_conventions_produce_the_same_published_y_up_map() {
    let mut session = export_fixture("normal", &[("intensity", Value::Float(8.0))], "normal");
    let graph = session.graph_mut().unwrap();
    let normal = graph.nodes[0].id;
    let input = node(graph, "input_grayscale", &[("identifier", Value::Text("slope".into()))]);
    graph.connect(input, "output", normal, "input").unwrap();
    let slope = Image::generate(32, 32, Kind::Gray, |_, y| [if y < 16 { 0.0 } else { 1.0 }; 4]);
    let png = encode(&slope, "png", Depth::U8, false).unwrap();
    let mut greens = Vec::new();

    for open_gl in [None, Some(false), Some(true)] {
        if let Some(open_gl) = open_gl {
            session.graph_mut().unwrap().node_mut(normal).unwrap().params.insert("opengl".into(), Value::Bool(open_gl));
        }
        let (files, report) = material_bake::export(&session, &json!({"profile":"materialx","inputs":{"slope":"input.png"}}), |_| Ok(png.clone())).unwrap();
        let bytes = &files.iter().find(|file| file.file == report["maps"][0]["file"]).unwrap().bytes;
        let pixel = materialcraft_formats::decode(bytes, true).unwrap().get(16, 16, false);

        assert_eq!(report["normalOrientation"], "Y+");
        greens.push(pixel[1]);
    }

    assert!(greens[2] > 0.8);
    assert!(greens.iter().all(|green| (green - greens[2]).abs() < 0.0001), "{greens:?}");
}

#[test]
fn raw_normal_inputs_accept_an_explicit_source_convention() {
    let session = export_fixture("input_color", &[("identifier", Value::Text("normal".into()))], "normal");
    let png = encode(&Image::filled(32, 32, Kind::Color, [0.5, 0.25, 1.0, 1.0]), "png", Depth::U16, false).unwrap();

    for (format, expected) in [(J::Null, 0.25), (json!("opengl"), 0.25), (json!("directx"), 0.75)] {
        let params = json!({"profile":"maps","inputs":{"normal":"normal.png"},"normalFormat":format});
        let (files, _) = material_bake::export(&session, &params, |_| Ok(png.clone())).unwrap();
        let green = materialcraft_formats::decode(&files[0].bytes, true).unwrap().get(0, 0, false)[1];
        assert!((green - expected).abs() < 0.0001);
    }
}

#[test]
fn selecting_a_healthy_output_excludes_other_cook_failures() {
    let mut session = export_fixture("uniform", &[], "baseColor");
    let graph = session.graph_mut().unwrap();
    let missing = node(graph, "bitmap", &[("resource", Value::Text("missing".into()))]);
    output(graph, missing, "output", "broken", "roughness");

    for profile in [J::Null, json!("maps"), json!("materialx")] {
        let (files, _) = material_bake::export(&session, &json!({"profile":profile,"outputs":["result"]}), material_bake::read_input).unwrap();
        assert!(!files.is_empty());

        assert!(material_bake::export(&session, &json!({"profile":profile,"outputs":["broken"]}), material_bake::read_input).is_err());
    }
}

#[test]
fn requested_bake_dimensions_apply_to_fixed_size_source_nodes() {
    let mut session = export_fixture("uniform", &[], "baseColor");
    let id = session.graph().unwrap().nodes[0].id;
    execute(&mut session, "graph.node.base", &json!({"id":id,"sizeMode":"absolute","size":[4,4]})).unwrap();

    let settings = json!({"profile":"materialx","size":[128,64]});
    let (files, report) = material_bake::export(&session, &settings, material_bake::read_input).unwrap();
    let pixels = materialcraft_formats::decode(&files[0].bytes, false).unwrap();

    assert_eq!([pixels.width(), pixels.height()], [128, 64]);
    assert_eq!(report["maps"][0]["size"], json!([128, 64]));
    assert_eq!(material_bake::cook(&session, &settings, material_bake::read_input).unwrap()["outputs"][0]["size"], json!([128, 64]));
}

#[test]
fn an_openpbr_scalar_rejects_an_unrepresentable_srgb_override() {
    let session = export_fixture("uniform", &[], "roughness");
    let settings = json!({"profile":"materialx","colorSpaces":{"result":"srgb"}});

    let result = material_bake::export(&session, &settings, material_bake::read_input);

    assert!(result.err().is_some_and(|error| error.to_string().contains("linear")));
}

#[test]
fn explicit_srgb_is_rejected_when_the_profile_requires_float_textures() {
    let session = export_fixture("uniform", &[], "baseColor");
    let automatic = json!({"profile":"materialx","depth":32});
    let explicit_srgb = json!({"profile":"materialx","depth":32,"colorSpaces":{"result":"srgb"}});

    let (_, report) = material_bake::export(&session, &automatic, material_bake::read_input).unwrap();
    let error = material_bake::export(&session, &explicit_srgb, material_bake::read_input).err().expect("an explicit sRGB override cannot produce linear EXR");

    assert_eq!(report["maps"][0]["colorSpace"], "linear");
    assert!(report["maps"][0]["file"].as_str().unwrap().ends_with(".exr"));
    assert!(error.to_string().contains("requires linear values"));
}

#[test]
fn profile_controls_fail_when_the_requested_export_cannot_apply_them() {
    let session = export_fixture("uniform", &[], "baseColor");

    for (params, option) in [
        (json!({"profile":"maps","displacementScale":2}), "displacementScale"),
        (json!({"profile":"maps","depth":8.4}), "depth"),
        (json!({"profile":"maps","depth":"mistyped"}), "depth"),
    ] {
        let error = material_bake::export(&session, &params, material_bake::read_input).err().expect("an unused option must be rejected");

        assert!(error.to_string().contains(option), "{error}");
    }
}

#[test]
fn legacy_grayscale_float_exports_keep_linear_defaults_and_explicit_overrides() {
    let session = export_fixture("uniform", &[("color", Value::Color([0.25; 4])), ("color_mode", Value::Bool(false))], "baseColor");
    for format in ["exr", "hdr"] {
        for explicit in [None, Some("raw"), Some("linear"), Some("srgb")] {
            let mut params = json!({"format": format});
            if let Some(space) = explicit {
                params["colorSpaces"] = json!({"result": space});
            }
            let (files, _) = material_bake::export(&session, &params, material_bake::read_input).unwrap();
            let decoded = materialcraft_formats::decode(&files[0].bytes, false).unwrap();
            let expected = if explicit == Some("srgb") { craft_texture::linear_to_srgb(0.25) } else { 0.25 };
            assert!((decoded.get(0, 0, false)[0] - expected).abs() < 0.005, "{format} {explicit:?}");
        }
    }
}

#[test]
fn published_disabled_and_typed_instance_cooks_match_source() {
    use materialcraft_core::ValueType;
    let mut helper = Graph::new("ValueHelper");
    helper.size = [4, 4];
    let value = node(&mut helper, "input_value", &[("identifier", Value::Text("amount".into())), ("default", Value::Float(0.25))]);
    output(&mut helper, value, "output", "number", "height");
    let mut dependency = Package::new("Dependency");
    dependency.graphs.push(helper);
    let mut graph = Graph::new("Root");
    graph.size = [4, 4];
    let value = node(&mut graph, "input_value", &[("identifier", Value::Text("source".into())), ("default", Value::Float(0.75))]);
    let instance = node(&mut graph, "graph_instance", &[]);
    graph.node_mut(instance).unwrap().instance = Some(GraphInstance {
        package: "helper.sbs".into(),
        graph: "ValueHelper".into(),
        inputs: vec![GraphPort { identifier: "amount".into(), type_code: ValueType::Float as u32 }],
        outputs: vec![GraphPort { identifier: "number".into(), type_code: ValueType::Float as u32 }],
        defaults: Default::default(),
        label: String::new(),
    });
    graph.connect(value, "output", instance, "amount").unwrap();
    output(&mut graph, instance, "number", "number", "height");
    let color = node(&mut graph, "uniform", &[("color", Value::Color([0.2, 0.3, 0.4, 1.0]))]);
    let levels = node(&mut graph, "levels", &[("out_low", Value::Float4([1.0; 4])), ("out_high", Value::Float4([0.0; 4]))]);
    graph.node_mut(levels).unwrap().disabled = true;
    graph.connect(color, "output", levels, "input").unwrap();
    output(&mut graph, levels, "output", "image", "baseColor");
    let mut package = Package::new("Package");
    package.graphs.push(graph);
    package.dependencies.insert("helper.sbs".into(), std::sync::Arc::new(dependency));
    let bytes = materialcraft_formats::material::write_material(&package, 0).unwrap();
    let (opened, entry) = materialcraft_formats::material::read_material(&bytes).unwrap();
    let cook = |pkg: Package, entry| {
        // Cook without Session::new because it can reconstruct a missing interface
        let options = materialcraft_eval::Options::new(&|_| None);
        let cache = std::sync::Mutex::new(materialcraft_eval::Cache::default());
        let cooked = materialcraft_eval::cook(&pkg, entry, &options, &cache);
        let graph = &pkg.graphs[entry];
        let instance = graph.nodes.iter().find(|node| node.def == "graph_instance").unwrap().id;
        let levels = graph.nodes.iter().find(|node| node.def == "levels").unwrap().id;
        assert!(cooked.nodes[&instance].error.is_none(), "{:?}", cooked.nodes[&instance].error);
        assert_eq!(cooked.nodes[&instance].outputs[0].as_value(), Some(&Value::Float(0.75)));
        assert!(cooked.nodes[&levels].error.is_none());
        cooked.image(levels, 0).unwrap().get(0, 0, false)
    };
    let original = cook(package, 0);
    let restored = cook(opened, entry);
    assert_eq!(restored, original);
    assert!((restored[0] - 0.2).abs() < 0.01, "disabled levels must preserve its input: {restored:?}");
}

#[test]
fn material_options_ignore_stale_presets_but_validate_explicit_values() {
    let mut session = fixture();
    let graph = session.graph_mut().unwrap();
    graph.presets[0].values.insert("removed".into(), Value::Float(0.5));
    graph.presets[0].values.insert("tint".into(), Value::Text("old type".into()));
    let before = graph.clone();
    let read = |_: &str| -> crate::Result<Vec<u8>> { Err(crate::EngineError::Other("unexpected image read".into())) };
    let options = MaterialOptions::from_params(graph, &json!({"preset":"Dark"}), read).unwrap();
    assert!(options.values.is_empty());
    let options = MaterialOptions::from_params(graph, &json!({"preset":"Dark", "values":{"tint":[0.7,0.6,0.5,1.0]}}), read).unwrap();
    assert_eq!(options.values.len(), 1);
    assert_eq!(options.values["tint"], Value::Color([0.7, 0.6, 0.5, 1.0]));
    assert!(MaterialOptions::from_params(graph, &json!({"values":{"removed":0.5}}), read).is_err());
    assert!(MaterialOptions::from_params(graph, &json!({"values":{"tint":"wrong"}}), read).is_err());
    assert_eq!(*graph, before);
}

#[test]
fn material_options_size_errors_name_the_material_command() {
    let session = fixture();
    let parameters = json!({"size":"bad"});
    let error =
        MaterialOptions::from_params(session.graph().unwrap(), &parameters, |_| Err(crate::EngineError::Other("unexpected image read".into()))).unwrap_err();
    assert!(matches!(error, crate::EngineError::BadParams { cmd, msg } if cmd == "material.cook" && msg.contains("size")));
    let error = crate::export_request::export_size("file.export", parameters.get("size")).unwrap_err();
    assert!(matches!(error, crate::EngineError::BadParams { cmd, msg } if cmd == "file.export" && msg.contains("size")));
}

#[test]
fn named_profiles_reject_ignored_format_and_pattern() {
    let session = fixture();
    for extra in [json!({"format":"jpg"}), json!({"pattern":"oak_$(identifier)"})] {
        let mut params = json!({"graph":"Materials/Oak","profile":"maps"});
        params.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        let err = material_bake::export(&session, &params, |_| panic!("invalid options must precede input reads")).err().expect("invalid export must fail");
        assert!(err.to_string().contains("omit format and pattern"));
    }
    let err =
        material_bake::export(&session, &json!({"graph":"Materials/Oak","normalFormat":"directx"}), |_| panic!("invalid options must precede input reads"))
            .err()
            .expect("invalid export must fail");
    assert!(err.to_string().contains("require an explicit export profile"));
}

#[test]
fn float_maps_reject_explicit_srgb_instead_of_ignoring_it() {
    let session = fixture();
    let params = json!({"graph":"Materials/Oak","profile":"maps","outputs":["albedo"],"depth":32,"colorSpaces":{"albedo":"srgb"}});
    let err = material_bake::export(&session, &params, material_bake::read_input).err().expect("invalid export must fail");
    assert!(err.to_string().contains("requires linear values"));
}

#[test]
fn named_profiles_report_the_validated_numeric_graph_selector() {
    let session = fixture();
    for profile in ["maps", "materialx"] {
        for selector in [json!(1.0), json!(0.6)] {
            let (files, report) =
                material_bake::export(&session, &json!({"graph":selector,"profile":profile,"outputs":["albedo"],"size":16}), material_bake::read_input)
                    .unwrap();
            assert!(!files.is_empty());
            assert_eq!(report["material"]["graph"], "Materials/Oak");
        }
    }
}
