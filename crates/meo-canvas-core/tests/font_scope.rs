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

#[test]
fn the_scope_is_the_thread_and_not_the_process() {
    // **This is the difference between a hazard and a catastrophe**, so it is
    // measured in both directions rather than inferred from the one above.
    // A pool of render threads has one copy of the problem each rather than
    // one shared between them, and each worker still needs its own start-up
    // registration.
    const IN_WORKER: &str = "MeoFontScopeWorkerProbe";
    const IN_MAIN: &str = "MeoFontScopeMainProbe";

    let seen_in_worker = std::thread::spawn(|| {
        let fonts = Fonts::new();
        fonts
            .register_path(IN_WORKER, FONT)
            .unwrap_or_else(|error| unreachable!("{error}"));
        // Visible to another registry on the same thread, which is the
        // thread-wide half of the claim.
        Fonts::new().has(IN_WORKER)
    })
    .join()
    .unwrap_or_else(|_| unreachable!("the worker did not panic"));
    assert!(
        seen_in_worker,
        "a registration was not visible on its own thread"
    );

    assert!(
        !Fonts::new().has(IN_WORKER),
        "a family registered on a worker reached the main thread, so the          registry is the process's after all -- the module doc says otherwise"
    );

    let owner = Fonts::new();
    owner
        .register_path(IN_MAIN, FONT)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let seen_in_a_later_thread =
        std::thread::spawn(|| Fonts::new().has(IN_MAIN))
            .join()
            .unwrap_or_else(|_| unreachable!("the worker did not panic"));
    assert!(
        !seen_in_a_later_thread,
        "a family registered on the main thread reached a thread spawned after          it, so the registry is the process's after all"
    );
}
