//! What registering a font actually changes.
//!
//! **The behaviour here looks impossible from the type**, which is why it is
//! pinned rather than described. `Fonts` is a value a caller holds, and it
//! reads as the scope of what it registers; the registry underneath it belongs
//! to the process, so a registration outlives the `Fonts` that made it and is
//! visible to every `Fonts` built afterwards.
//!
//! One test rather than several, and deliberately: these assertions are about
//! one process, and splitting them across tests would let the harness run them
//! on different threads in an order that decides the answer.
use meo_canvas_core::{Fonts, Renderer};
use meo_canvas_scene::{Scene, Size, node::Node};

const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/fonts/Oswald-VariableFont_wght.ttf"
);

/// A family this test registers and nothing else in the workspace uses, so the
/// `false` at the start is not a claim about test ordering.
const FAMILY: &str = "MeoFontScopeProbe";

fn page(family: &str) -> Scene {
    let mut scene = Scene::new(Size::new(200.0, 60.0));
    let root = scene
        .root()
        .unwrap_or_else(|| unreachable!("a new scene has a root"));
    let mut node = Node::text("Hello");
    node.text.font_family = Some(family.to_owned());
    node.text.font_size = Some(24.0);
    scene
        .push(root, node)
        .unwrap_or_else(|error| unreachable!("{error}"));
    scene
}

#[test]
fn a_registration_outlives_the_registry_that_made_it() {
    // Nothing has registered it yet, which is what makes the rest a
    // measurement rather than a coincidence.
    assert!(
        !Fonts::new().has(FAMILY),
        "{FAMILY} was already registered; pick a family nothing else uses"
    );

    {
        // `register_path` takes `&self`, which is the API saying what the
        // module doc now says: registering is not a mutation of this value.
        let owner = Fonts::new();
        owner
            .register_path(FAMILY, FONT)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(owner.registered(), vec![FAMILY.to_owned()]);
    }
    // `owner` is gone. Everything below is about a process it changed.

    let after = Fonts::new();
    assert!(
        after.has(FAMILY),
        "a registry built after the owner was dropped cannot draw the family, \
         so the registration is scoped after all -- update the module doc"
    );
    assert!(
        after.registered().is_empty(),
        "`registered` reported a family this registry did not register"
    );

    // **The two accessors disagree, on purpose.** `has` answers "can this be
    // drawn in this process" and `registered` answers "what did I register".
    // A caller asking both of a fresh registry sees an inconsistent library
    // and is looking at two correct answers to two different questions.
    assert!(after.has(FAMILY) && after.registered().is_empty());

    // And the render that matters: a `Renderer` built after the drop, holding
    // a registry that never saw the face, draws with it anyway.
    let renderer = Renderer::new();
    assert!(
        renderer.render(&page(FAMILY)).is_ok(),
        "the face did not survive into a later renderer"
    );

    // The check still discriminates -- this is not "everything renders".
    assert!(
        renderer.render(&page("MeoNoSuchFamilyAnywhere")).is_err(),
        "an unregistered family rendered, so the assertion above proves nothing"
    );
}
