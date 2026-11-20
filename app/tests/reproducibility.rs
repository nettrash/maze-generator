//! Pins the maze each seed draws.
//!
//! The README promises that the same seed, size and algorithm always give the
//! same maze, and a saved seed is only worth keeping while that holds. A
//! refactor of an algorithm that changes the order it draws in, or a new
//! data structure that visits cells differently, keeps every maze valid and
//! every property test green while redrawing every seeded maze, so the mazes
//! themselves are pinned here.
//!
//! Each entry is the FNV-1a 64 hash of [`Map::to_text`], which is exactly
//! what `maze --command generate --output -` prints. Plain and opened mazes
//! were recorded from the 0.0.2 release and have not changed since, except
//! Eller's: 0.0.3 stopped it carving the last column into one straight
//! corridor, which redrew every Eller maze. Braided mazes changed in 0.0.3 as
//! well, when braiding stopped drawing from the algorithm's own stream and
//! stopped opening dead ends that were already joined. Both were recorded
//! again then.
//!
//! Changing what a seed draws is sometimes the right call, but it is a
//! user-visible change: regenerate the affected rows and say so in the
//! CHANGELOG.

use maze::{Generator, Map, MazeOptions};

/// `(algorithm, width, height, seed, FNV-1a 64 of the saved document)`.
type Pin = (&'static str, u64, u64, u64, u64);

/// A shape one cell wide or one cell tall has a single perfect maze, a
/// straight corridor, so the seed cannot matter there; one seed per shape is
/// enough.
const DEGENERATE: &[Pin] = &[
    ("backtracking", 1, 1, 7, 0x019e56837afd9c76),
    ("backtracking", 1, 9, 7, 0x2e094fffb029d12e),
    ("backtracking", 9, 1, 7, 0x9b5bd7acd635209e),
    ("prim", 1, 1, 7, 0x019e56837afd9c76),
    ("prim", 1, 9, 7, 0x2e094fffb029d12e),
    ("prim", 9, 1, 7, 0x9b5bd7acd635209e),
    ("kruskal", 1, 1, 7, 0x019e56837afd9c76),
    ("kruskal", 1, 9, 7, 0x2e094fffb029d12e),
    ("kruskal", 9, 1, 7, 0x9b5bd7acd635209e),
    ("aldous_broder", 1, 1, 7, 0x019e56837afd9c76),
    ("aldous_broder", 1, 9, 7, 0x2e094fffb029d12e),
    ("aldous_broder", 9, 1, 7, 0x9b5bd7acd635209e),
    ("wilson", 1, 1, 7, 0x019e56837afd9c76),
    ("wilson", 1, 9, 7, 0x2e094fffb029d12e),
    ("wilson", 9, 1, 7, 0x9b5bd7acd635209e),
    ("eller", 1, 1, 7, 0x019e56837afd9c76),
    ("eller", 1, 9, 7, 0x2e094fffb029d12e),
    ("eller", 9, 1, 7, 0x9b5bd7acd635209e),
];

const PLAIN: &[Pin] = &[
    ("backtracking", 2, 2, 0, 0xeb631adc862c536f),
    ("backtracking", 2, 2, 1, 0xeb631adc862c536f),
    ("backtracking", 2, 2, 42, 0xeb631adc862c536f),
    ("backtracking", 2, 2, u64::MAX, 0x764725b52a5a8175),
    ("backtracking", 7, 5, 0, 0xf0f2cf866093add2),
    ("backtracking", 7, 5, 1, 0xd3da0b89ae4e8df8),
    ("backtracking", 7, 5, 42, 0x803c19ad110e84e4),
    ("backtracking", 7, 5, u64::MAX, 0x78a7aa824837f5a6),
    ("backtracking", 16, 9, 0, 0x4ea65b6826f497c0),
    ("backtracking", 16, 9, 1, 0xfb8670aed8b753f2),
    ("backtracking", 16, 9, 42, 0x76c64a01b165389a),
    ("backtracking", 16, 9, u64::MAX, 0x082d24e5282f0570),
    ("backtracking", 40, 25, 0, 0x245d9a0ef4e2728d),
    ("backtracking", 40, 25, 1, 0x20ca6bef5d0bd8b7),
    ("backtracking", 40, 25, 42, 0x05dd83800b56b6b3),
    ("backtracking", 40, 25, u64::MAX, 0xafd893bd0dfcfa69),
    ("backtracking", 3, 64, 0, 0x0349bd8e01f2e45d),
    ("backtracking", 3, 64, 1, 0x87da1d02eff1f0cd),
    ("backtracking", 3, 64, 42, 0xef09dd35e874db09),
    ("backtracking", 3, 64, u64::MAX, 0xed27e804e8464e05),
    ("backtracking", 64, 3, 0, 0x9e6479bb4a53d7f1),
    ("backtracking", 64, 3, 1, 0x8967836bf1019d3f),
    ("backtracking", 64, 3, 42, 0x3b105aa4d86cec97),
    ("backtracking", 64, 3, u64::MAX, 0x0ee0258b82a2be3b),
    ("prim", 2, 2, 0, 0xeea83c2d9faee827),
    ("prim", 2, 2, 1, 0xeea83c2d9faee827),
    ("prim", 2, 2, 42, 0xeb631adc862c536f),
    ("prim", 2, 2, u64::MAX, 0xca6f09bed99db789),
    ("prim", 7, 5, 0, 0x3d6cc32045d7cd8e),
    ("prim", 7, 5, 1, 0x3df337eb5e281b54),
    ("prim", 7, 5, 42, 0xf0d738b9eac28238),
    ("prim", 7, 5, u64::MAX, 0x2cd6f43f4ffcf722),
    ("prim", 16, 9, 0, 0x465b392f0cb74c0e),
    ("prim", 16, 9, 1, 0xc06a6922a5f829de),
    ("prim", 16, 9, 42, 0x85e5caf44f61a524),
    ("prim", 16, 9, u64::MAX, 0x4d31867819b58336),
    ("prim", 40, 25, 0, 0x9c16f2058c9ac647),
    ("prim", 40, 25, 1, 0x1014a5d2553492eb),
    ("prim", 40, 25, 42, 0x157b060f737908fd),
    ("prim", 40, 25, u64::MAX, 0xd2b544828f5889c5),
    ("prim", 3, 64, 0, 0xb63730daec8cbc6f),
    ("prim", 3, 64, 1, 0x3dab98a60503808b),
    ("prim", 3, 64, 42, 0x026ac39033d25883),
    ("prim", 3, 64, u64::MAX, 0xeb7af5e7fd3eb341),
    ("prim", 64, 3, 0, 0x2adf8f2a46f8ff01),
    ("prim", 64, 3, 1, 0xe2f8306bc8228c8d),
    ("prim", 64, 3, 42, 0x80317d7a6d8bde35),
    ("prim", 64, 3, u64::MAX, 0x54645ee082a1b809),
    ("kruskal", 2, 2, 0, 0xeea83c2d9faee827),
    ("kruskal", 2, 2, 1, 0x764725b52a5a8175),
    ("kruskal", 2, 2, 42, 0x764725b52a5a8175),
    ("kruskal", 2, 2, u64::MAX, 0xeb631adc862c536f),
    ("kruskal", 7, 5, 0, 0x0858bfb5e18ef4bc),
    ("kruskal", 7, 5, 1, 0x7174833a303bb7b2),
    ("kruskal", 7, 5, 42, 0xfef81afec0499d9a),
    ("kruskal", 7, 5, u64::MAX, 0xcf38eb2c700803f4),
    ("kruskal", 16, 9, 0, 0xb0a2433ad6715f2e),
    ("kruskal", 16, 9, 1, 0x5708ee30bd2e2aa6),
    ("kruskal", 16, 9, 42, 0x86192105e6b36f6e),
    ("kruskal", 16, 9, u64::MAX, 0xb403f8cc01e370ea),
    ("kruskal", 40, 25, 0, 0x882d92b93e3addb5),
    ("kruskal", 40, 25, 1, 0xbf022c1917cd13af),
    ("kruskal", 40, 25, 42, 0xef7a1395fea9cd11),
    ("kruskal", 40, 25, u64::MAX, 0x2eec6b69614afedf),
    ("kruskal", 3, 64, 0, 0x1280911900c44801),
    ("kruskal", 3, 64, 1, 0xe206e56126c3827d),
    ("kruskal", 3, 64, 42, 0x086347c8d03cfb7d),
    ("kruskal", 3, 64, u64::MAX, 0x38c2b73515d195f9),
    ("kruskal", 64, 3, 0, 0x411f0bafa428fc7b),
    ("kruskal", 64, 3, 1, 0x52d4decf9eef221f),
    ("kruskal", 64, 3, 42, 0xac7efd65c24ab0a5),
    ("kruskal", 64, 3, u64::MAX, 0xce995f81ae3d9fa1),
    ("aldous_broder", 2, 2, 0, 0xca6f09bed99db789),
    ("aldous_broder", 2, 2, 1, 0xeea83c2d9faee827),
    ("aldous_broder", 2, 2, 42, 0xeb631adc862c536f),
    ("aldous_broder", 2, 2, u64::MAX, 0x764725b52a5a8175),
    ("aldous_broder", 7, 5, 0, 0x14bade7938f0536a),
    ("aldous_broder", 7, 5, 1, 0x71f2696e1833079e),
    ("aldous_broder", 7, 5, 42, 0x0f78c31b18ae4d86),
    ("aldous_broder", 7, 5, u64::MAX, 0xdcbf6885eb4b5b42),
    ("aldous_broder", 16, 9, 0, 0xa1ab86d00544f2ce),
    ("aldous_broder", 16, 9, 1, 0x181595a8beed6948),
    ("aldous_broder", 16, 9, 42, 0x9cec6096a2721c10),
    ("aldous_broder", 16, 9, u64::MAX, 0xf8b29599730bba02),
    ("aldous_broder", 40, 25, 0, 0xd324d5e261d5aaa7),
    ("aldous_broder", 40, 25, 1, 0x076ba91aba9926b9),
    ("aldous_broder", 40, 25, 42, 0xe79da809c79df831),
    ("aldous_broder", 40, 25, u64::MAX, 0xde18c08934238d4d),
    ("aldous_broder", 3, 64, 0, 0x0da9d98d237f2a9f),
    ("aldous_broder", 3, 64, 1, 0x08af6408155332cb),
    ("aldous_broder", 3, 64, 42, 0x5d37631702db470f),
    ("aldous_broder", 3, 64, u64::MAX, 0x1590c8b60598f0c7),
    ("aldous_broder", 64, 3, 0, 0xa2703cb9c3a0f8cd),
    ("aldous_broder", 64, 3, 1, 0xe1c5ef3b56d3d8c3),
    ("aldous_broder", 64, 3, 42, 0x62c95913cfd32489),
    ("aldous_broder", 64, 3, u64::MAX, 0x592dcc070238b787),
    ("wilson", 2, 2, 0, 0xca6f09bed99db789),
    ("wilson", 2, 2, 1, 0xeb631adc862c536f),
    ("wilson", 2, 2, 42, 0xeea83c2d9faee827),
    ("wilson", 2, 2, u64::MAX, 0x764725b52a5a8175),
    ("wilson", 7, 5, 0, 0xa3f952d4c5e66df4),
    ("wilson", 7, 5, 1, 0x197c644149b38512),
    ("wilson", 7, 5, 42, 0x509f5265b6fdd468),
    ("wilson", 7, 5, u64::MAX, 0x138c07506b979928),
    ("wilson", 16, 9, 0, 0x6237e075497af9b4),
    ("wilson", 16, 9, 1, 0x6af9fa294d42c0dc),
    ("wilson", 16, 9, 42, 0x8645e6dde11a66e2),
    ("wilson", 16, 9, u64::MAX, 0x2492092de15e970a),
    ("wilson", 40, 25, 0, 0xb0260c3f916d196f),
    ("wilson", 40, 25, 1, 0x0a7519874313e8df),
    ("wilson", 40, 25, 42, 0x271887942b02e31d),
    ("wilson", 40, 25, u64::MAX, 0x4e461f6a2b03d75f),
    ("wilson", 3, 64, 0, 0xead4156f626cab3d),
    ("wilson", 3, 64, 1, 0x3dc8cf28430f2583),
    ("wilson", 3, 64, 42, 0x66d4089241a1918d),
    ("wilson", 3, 64, u64::MAX, 0x4696391bee730931),
    ("wilson", 64, 3, 0, 0xd61ba5a12cc6b0f3),
    ("wilson", 64, 3, 1, 0x7b637e9cfb94b2b3),
    ("wilson", 64, 3, 42, 0x485c83971c66d3fb),
    ("wilson", 64, 3, u64::MAX, 0xfa1c0cc2bbc8cd51),
    ("eller", 2, 2, 0, 0x764725b52a5a8175),
    ("eller", 2, 2, 1, 0xca6f09bed99db789),
    ("eller", 2, 2, 42, 0xca6f09bed99db789),
    ("eller", 2, 2, u64::MAX, 0xeb631adc862c536f),
    ("eller", 7, 5, 0, 0x8297aa3ea79f2702),
    ("eller", 7, 5, 1, 0x2eedf7af5271bf86),
    ("eller", 7, 5, 42, 0x351e09a64593b2fe),
    ("eller", 7, 5, u64::MAX, 0x07988156e0493156),
    ("eller", 16, 9, 0, 0xdce0354738382bd2),
    ("eller", 16, 9, 1, 0x5779fb427f23d7d0),
    ("eller", 16, 9, 42, 0x7e8a1b6d4112c6ce),
    ("eller", 16, 9, u64::MAX, 0x2c7608d7fed74bbe),
    ("eller", 40, 25, 0, 0x8956578f2321e671),
    ("eller", 40, 25, 1, 0xee19600b5ee52721),
    ("eller", 40, 25, 42, 0xfa351dd131b2d77d),
    ("eller", 40, 25, u64::MAX, 0xc4bf2855dba56d9b),
    ("eller", 3, 64, 0, 0x97d1757095864bff),
    ("eller", 3, 64, 1, 0x3764ba4cfb8739d9),
    ("eller", 3, 64, 42, 0x16fc930e38554efb),
    ("eller", 3, 64, u64::MAX, 0xccd2cf40e5a0aa93),
    ("eller", 64, 3, 0, 0xd5c5ba132aee69a7),
    ("eller", 64, 3, 1, 0x418c1122bc0ff4bf),
    ("eller", 64, 3, 42, 0x57564e71229dcde5),
    ("eller", 64, 3, u64::MAX, 0x6c806f2614c95983),
];

/// The entrance and exit add no randomness, so these must match the plain
/// mazes' history exactly.
const OPENED: &[Pin] = &[
    ("backtracking", 7, 5, 1, 0x6bb046e9baf49f50),
    ("backtracking", 16, 9, 1, 0x1866bbb045f1bb2a),
    ("prim", 7, 5, 1, 0xc536bb448a091044),
    ("prim", 16, 9, 1, 0x35932f3eb85f9e6e),
    ("kruskal", 7, 5, 1, 0xaa9846f54c029cc2),
    ("kruskal", 16, 9, 1, 0xffac60de057ee436),
    ("aldous_broder", 7, 5, 1, 0xfe3b0ecaa60c6e26),
    ("aldous_broder", 16, 9, 1, 0x14924b08d1e1e638),
    ("wilson", 7, 5, 1, 0x0ea5ce54d9be95e2),
    ("wilson", 16, 9, 1, 0x6e0a6944666028f4),
    ("eller", 7, 5, 1, 0xed0888851accc11e),
    ("eller", 16, 9, 1, 0xcebb6aa26a015c20),
];

const BRAIDED: &[Pin] = &[
    ("backtracking", 7, 5, 1, 0x78485d8a40ebd5d4),
    ("backtracking", 7, 5, 42, 0x179c254edfa498ac),
    ("backtracking", 16, 9, 1, 0xff11a3192cdd2892),
    ("backtracking", 16, 9, 42, 0x171070420006278a),
    ("backtracking", 40, 25, 1, 0x9cbd6f4c0eabbc9d),
    ("backtracking", 40, 25, 42, 0xf1020df2f7c148db),
    ("prim", 7, 5, 1, 0xbd6279b12601024c),
    ("prim", 7, 5, 42, 0x78811d7a2685dd2e),
    ("prim", 16, 9, 1, 0x3e07df831875156e),
    ("prim", 16, 9, 42, 0x4655d7208ebdeae2),
    ("prim", 40, 25, 1, 0x6e0cd03445af8abf),
    ("prim", 40, 25, 42, 0xd73a09f524575c6d),
    ("kruskal", 7, 5, 1, 0x2b6a2aa53b55bf08),
    ("kruskal", 7, 5, 42, 0xc91d6fcc8f18102c),
    ("kruskal", 16, 9, 1, 0xffd350f39c8f28cc),
    ("kruskal", 16, 9, 42, 0x19fb63d812ea0a08),
    ("kruskal", 40, 25, 1, 0xf6a667419a10d8ff),
    ("kruskal", 40, 25, 42, 0x686939cb4bff80f3),
    ("aldous_broder", 7, 5, 1, 0x50b1274265b4be62),
    ("aldous_broder", 7, 5, 42, 0xf2ff4eef2468292a),
    ("aldous_broder", 16, 9, 1, 0x4f4cf0b1a1b64f90),
    ("aldous_broder", 16, 9, 42, 0x8bf28456f17d1688),
    ("aldous_broder", 40, 25, 1, 0xd5e850a674b726fb),
    ("aldous_broder", 40, 25, 42, 0x457062475d3c855f),
    ("wilson", 7, 5, 1, 0xb3543c803dbfb4ea),
    ("wilson", 7, 5, 42, 0x2457298122e23b56),
    ("wilson", 16, 9, 1, 0x44a072e1e96527da),
    ("wilson", 16, 9, 42, 0x96af79a4fb2d045a),
    ("wilson", 40, 25, 1, 0xba49cb8c4618a96d),
    ("wilson", 40, 25, 42, 0xa37794bb46318df7),
    ("eller", 7, 5, 1, 0xc84403c7713f5e18),
    ("eller", 7, 5, 42, 0x5c8f25f7a04ebac4),
    ("eller", 16, 9, 1, 0xc5b7f8318dbed0ae),
    ("eller", 16, 9, 42, 0x129cc94b4c2fba84),
    ("eller", 40, 25, 1, 0x8f55b0a9926bb459),
    ("eller", 40, 25, 42, 0x8c34c7876aa29f47),
];

/// FNV-1a 64: tiny, dependency free and stable, which is all a pin needs.
fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn check(pins: &[Pin], options: MazeOptions) {
    let mut redrawn = Vec::new();
    for &(algorithm, width, height, seed, expected) in pins {
        let map: Map = Generator::new(width, height, algorithm, Some(seed))
            .expect("pinned algorithms are known")
            .with_options(options)
            .build();
        let actual = fnv1a(&map.to_text());
        if actual != expected {
            redrawn.push(format!(
                "{algorithm} {width}x{height} seed {seed}: expected {expected:#018x}, got {actual:#018x}"
            ));
        }
    }
    assert!(
        redrawn.is_empty(),
        "{} of {} pinned mazes are drawn differently now:\n{}",
        redrawn.len(),
        pins.len(),
        redrawn.join("\n")
    );
}

#[test]
fn seeds_still_draw_the_mazes_they_always_have() {
    check(DEGENERATE, MazeOptions::default());
    check(PLAIN, MazeOptions::default());
}

#[test]
fn opening_the_border_draws_nothing_extra() {
    check(
        OPENED,
        MazeOptions {
            braid: false,
            entrance_exit: true,
        },
    );
}

#[test]
fn braided_seeds_still_draw_the_mazes_they_have_since_0_0_3() {
    check(
        BRAIDED,
        MazeOptions {
            braid: true,
            entrance_exit: false,
        },
    );
}

/// The hash itself is pinned, since a change to it would make every table
/// above fail for no reason.
#[test]
fn the_hash_is_fnv1a_64() {
    assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a("foobar"), 0x8594_4171_f739_67e8);
}
