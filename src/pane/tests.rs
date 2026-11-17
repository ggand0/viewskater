use super::*;

fn pane(ctx: &egui::Context) -> Pane {
    Pane::new(ctx, 2, 64, 1, false, true, 0)
}

fn fake_paths(n: usize) -> Vec<PathBuf> {
    (0..n).map(|i| PathBuf::from(format!("/nonexistent/f{i}.png"))).collect()
}

/// A pane over fake paths with no caches: exercises the list and index
/// rules on their own. `load_sync` fails quietly on the fake files.
fn pane_with(ctx: &egui::Context, n: usize, current: usize) -> Pane {
    let mut p = pane(ctx);
    p.image_paths = fake_paths(n);
    p.current_index = current;
    p
}

fn accept(_: &Path) -> Result<(), String> {
    Ok(())
}

#[test]
fn rotation_cycles_in_both_directions() {
    assert_eq!(Rotation::None.clockwise(), Rotation::Clockwise90);
    assert_eq!(Rotation::Clockwise270.clockwise(), Rotation::None);
    assert_eq!(Rotation::None.counter_clockwise(), Rotation::Clockwise270);
    assert_eq!(Rotation::Clockwise90.counter_clockwise(), Rotation::None);
}

#[test]
fn quarter_turn_swaps_the_display_size() {
    let size = egui::vec2(400.0, 200.0);
    assert_eq!(Rotation::None.size(size), size);
    assert_eq!(Rotation::Clockwise90.size(size), egui::vec2(200.0, 400.0));
    assert_eq!(Rotation::Clockwise180.size(size), size);
    assert_eq!(Rotation::Clockwise270.size(size), egui::vec2(200.0, 400.0));
}

#[test]
fn clockwise_turn_moves_the_top_left_corner_to_the_top_right() {
    assert_eq!(
        Rotation::Clockwise90.rotate_vec(egui::vec2(-1.0, -1.0)),
        egui::vec2(1.0, -1.0)
    );
    assert_eq!(
        Rotation::Clockwise270.rotate_vec(egui::vec2(-1.0, -1.0)),
        egui::vec2(-1.0, 1.0)
    );
}

#[test]
fn changing_image_resets_rotation() {
    let ctx = egui::Context::default();
    let mut p = pane_with(&ctx, 3, 0);
    p.rotation = Rotation::Clockwise90;

    p.jump_to(1, &ctx);

    assert_eq!(p.rotation, Rotation::None);
}

#[test]
fn empty_pane_removes_nothing() {
    let ctx = egui::Context::default();
    let mut p = pane(&ctx);
    let mut called = false;
    let r = p.remove_current(&ctx, |_| -> Result<(), ()> {
        called = true;
        Ok(())
    });
    assert_eq!(r, Ok(None));
    assert!(!called, "trasher must not run for an empty pane");
}

/// The safety promise. The trash function is a parameter of
/// `remove_current`, so this test passes one that always fails, standing
/// in for the trash crate refusing (read-only share, no trash folder).
/// After the failure the list, the index and the image on screen must be
/// exactly what they were: the pane must never drop a file it did not
/// actually move.
#[test]
fn failed_move_leaves_pane_untouched() {
    let ctx = egui::Context::default();
    let mut p = pane_with(&ctx, 5, 2);
    let before = p.image_paths.clone();

    let r = p.remove_current(&ctx, |_| Err("no trash here".to_string()));

    assert_eq!(r, Err("no trash here".to_string()));
    assert_eq!(p.image_paths, before);
    assert_eq!(p.current_index, 2);
}

#[test]
fn trasher_gets_the_current_path() {
    let ctx = egui::Context::default();
    let mut p = pane_with(&ctx, 5, 3);
    let mut seen = None;
    let r = p.remove_current(&ctx, |path| -> Result<(), ()> {
        seen = Some(path.to_path_buf());
        Ok(())
    });
    assert_eq!(seen.as_deref(), Some(Path::new("/nonexistent/f3.png")));
    assert_eq!(r, Ok(Some(PathBuf::from("/nonexistent/f3.png"))));
}

#[test]
fn remove_middle_shows_the_next_file() {
    let ctx = egui::Context::default();
    let mut p = pane_with(&ctx, 5, 2);

    p.remove_current(&ctx, accept).unwrap();

    assert_eq!(p.image_paths, fake_paths(5).into_iter().filter(|x| x != Path::new("/nonexistent/f2.png")).collect::<Vec<_>>());
    assert_eq!(p.current_index, 2, "index stays, now naming the next file");
    assert_eq!(p.image_paths[2], PathBuf::from("/nonexistent/f3.png"));
}

#[test]
fn remove_first_shows_the_new_first() {
    let ctx = egui::Context::default();
    let mut p = pane_with(&ctx, 5, 0);
    p.remove_current(&ctx, accept).unwrap();
    assert_eq!(p.current_index, 0);
    assert_eq!(p.image_paths[0], PathBuf::from("/nonexistent/f1.png"));
    assert_eq!(p.image_paths.len(), 4);
}

#[test]
fn remove_last_steps_back() {
    let ctx = egui::Context::default();
    let mut p = pane_with(&ctx, 5, 4);
    p.remove_current(&ctx, accept).unwrap();
    assert_eq!(p.current_index, 3);
    assert_eq!(p.image_paths.len(), 4);
    assert_eq!(p.image_paths[3], PathBuf::from("/nonexistent/f3.png"));
}

#[test]
fn remove_only_file_empties_the_pane() {
    let ctx = egui::Context::default();
    let mut p = pane_with(&ctx, 1, 0);
    p.dir_path = Some(PathBuf::from("/nonexistent"));
    p.remove_current(&ctx, accept).unwrap();
    assert!(p.image_paths.is_empty());
    assert_eq!(p.current_index, 0);
    assert!(p.current_texture.is_none());
    assert_eq!(p.dir_path.as_deref(), Some(Path::new("/nonexistent")), "directory stays known");
}

#[test]
fn remove_before_current_keeps_the_same_image() {
    let ctx = egui::Context::default();
    let mut p = pane_with(&ctx, 5, 3);
    p.remove_index(1, &ctx);
    assert_eq!(p.current_index, 2);
    assert_eq!(p.image_paths[p.current_index], PathBuf::from("/nonexistent/f3.png"));
}

#[test]
fn remove_after_current_changes_nothing_visible() {
    let ctx = egui::Context::default();
    let mut p = pane_with(&ctx, 5, 1);
    p.remove_index(4, &ctx);
    assert_eq!(p.current_index, 1);
    assert_eq!(p.image_paths.len(), 4);
}

#[test]
fn remove_out_of_range_is_a_no_op() {
    let ctx = egui::Context::default();
    let mut p = pane_with(&ctx, 3, 1);
    p.remove_index(3, &ctx);
    assert_eq!(p.image_paths.len(), 3);
    assert_eq!(p.current_index, 1);
}

#[test]
fn repeated_removal_walks_the_whole_list() {
    let ctx = egui::Context::default();
    let mut p = pane_with(&ctx, 6, 2);
    let mut removed = Vec::new();
    while !p.image_paths.is_empty() {
        let path = p.remove_current(&ctx, accept).unwrap().unwrap();
        removed.push(path);
        assert!(p.image_paths.is_empty() || p.current_index < p.image_paths.len());
    }
    // f2, f3, f4, f5 forward, then f1, f0 stepping back from the end.
    let names: Vec<_> = removed.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["f2.png", "f3.png", "f4.png", "f5.png", "f1.png", "f0.png"]);
}

// ---- with real files and live caches ----------------------------------

fn write_png(path: &Path, shade: u8) {
    let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([shade, shade, shade, 255]));
    img.save(path).unwrap();
}

/// Wait until every window slot around the current index is loaded or
/// the deadline passes; background decodes run on threads.
fn settle(p: &mut Pane) {
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    loop {
        p.poll_cache();
        let all_loaded = p.cache.as_ref().is_some_and(|c| {
            (0..p.image_paths.len()).all(|i| {
                c.loaded_for(i).is_some()
                    || c.summary().is_empty()
                    || !(c.first_file_index_for_test()..c.first_file_index_for_test() + 5).contains(&i)
            })
        });
        if all_loaded || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The full culling loop on real PNG files with the live caches.
///
/// Situation: a folder of six images opened at index 2. The trash
/// function is a rename into a second tempdir ("bin"), which is what the
/// trash crate does on the same filesystem, so nothing touches the real
/// trash.
///
/// Checks, in order: the file left its folder and arrived in the bin; no
/// other file moved; the list shrank by one; the same index now names the
/// next file and it is on screen. Then it jumps to the end and deletes
/// until the folder is empty, checking each step shows an image, to
/// exercise the step-back-at-end and empty-pane paths.
#[test]
fn real_files_move_out_and_the_pane_follows() {
    let ctx = egui::Context::default();
    let dir = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    for i in 0..6 {
        write_png(&dir.path().join(format!("img{i}.png")), (i * 40) as u8);
    }
    let mut p = pane(&ctx);
    p.open_path(&dir.path().join("img2.png"), &ctx, Default::default());
    assert_eq!(p.image_paths.len(), 6);
    assert_eq!(p.current_index, 2);
    settle(&mut p);

    // The test trasher is a rename into a "bin" directory, which is
    // what the trash crate does on the same filesystem.
    let bin_path = bin.path().to_path_buf();
    let move_out = |path: &Path| -> Result<(), String> {
        std::fs::rename(path, bin_path.join(path.file_name().unwrap())).map_err(|e| e.to_string())
    };

    let removed = p.remove_current(&ctx, move_out).unwrap().unwrap();
    assert_eq!(removed.file_name().unwrap(), "img2.png");
    assert!(!removed.exists(), "file left its directory");
    assert!(bin.path().join("img2.png").exists(), "file arrived in the bin");
    assert_eq!(p.image_paths.len(), 5);
    assert!(!p.image_paths.contains(&removed));
    assert_eq!(p.current_index, 2);
    assert_eq!(p.image_paths[2].file_name().unwrap(), "img3.png");
    assert!(p.current_texture.is_some(), "next image is on screen");
    let on_disk: Vec<_> = {
        let mut v: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        v.sort();
        v
    };
    assert_eq!(on_disk.len(), 5, "only the trashed file left the directory");

    // Keep going to the end and step back; textures must exist each time.
    settle(&mut p);
    for expected_len in (1..5).rev() {
        p.jump_to(p.image_paths.len() - 1, &ctx);
        settle(&mut p);
        p.remove_current(&ctx, move_out).unwrap().unwrap();
        assert_eq!(p.image_paths.len(), expected_len);
        assert_eq!(p.current_index, expected_len - 1);
        assert!(p.current_texture.is_some());
    }
    p.remove_current(&ctx, move_out).unwrap().unwrap();
    assert!(p.image_paths.is_empty());
    assert!(p.cache.is_none());
    assert_eq!(std::fs::read_dir(bin.path()).unwrap().count(), 6);
}

#[test]
fn real_files_failed_move_keeps_everything() {
    let ctx = egui::Context::default();
    let dir = tempfile::tempdir().unwrap();
    for i in 0..3 {
        write_png(&dir.path().join(format!("img{i}.png")), (i * 80) as u8);
    }
    let mut p = pane(&ctx);
    p.open_path(dir.path(), &ctx, Default::default());
    settle(&mut p);
    let before = p.image_paths.clone();

    let r = p.remove_current(&ctx, |_| Err("refused".to_string()));

    assert!(r.is_err());
    assert_eq!(p.image_paths, before);
    assert_eq!(p.current_index, 0);
    assert!(p.current_texture.is_some());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
}

/// Poll until the sliding window has nothing decoding or waiting for
/// upload, or the deadline passes.
fn wait_for_decodes(p: &mut Pane) {
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    loop {
        p.poll_cache();
        if p.is_settled() || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// A file that cannot be decoded sits between two that can. Keyboard
/// navigation moves only when the next image is ready, and a failed
/// decode counts as ready, so the arrow keys move onto the broken file
/// and past it in both directions. Before, the pane waited on that file
/// for good and the rest of the folder could not be reached.
#[test]
fn keyboard_navigation_passes_a_file_that_fails_to_decode() {
    let dir = tempfile::tempdir().unwrap();
    write_png(&dir.path().join("a.png"), 10);
    std::fs::write(dir.path().join("b.jpg"), b"this is not a jpeg").unwrap();
    write_png(&dir.path().join("c.png"), 30);

    let ctx = egui::Context::default();
    let mut p = pane(&ctx);
    p.open_path(&dir.path().join("a.png"), &ctx, Default::default());
    wait_for_decodes(&mut p);
    assert_eq!(p.current_index, 0);

    assert!(p.is_next_cached(1));
    assert!(p.navigate(1, &ctx));
    assert_eq!(p.current_index, 1);
    assert!(p.current_texture.is_none());
    assert_eq!(p.current_record.as_ref().and_then(|r| r.file_size), Some(18));

    assert!(p.is_next_cached(1));
    assert!(p.navigate(1, &ctx));
    assert_eq!(p.current_index, 2);
    assert!(p.current_texture.is_some());

    assert!(p.navigate(-1, &ctx));
    assert_eq!(p.current_index, 1);
    assert!(p.current_texture.is_none());
    assert!(p.navigate(-1, &ctx));
    assert_eq!(p.current_index, 0);
    assert!(p.current_texture.is_some());
}
