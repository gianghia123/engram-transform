#![no_std]
extern crate alloc;

use alloc::vec::Vec;
use ark_bn254::{Fr, FrConfig};
use ark_ff::{BigInt, Field, Fp, MontBackend, MontFp, PrimeField, Zero, BigInteger};
use engram_shared::{error::*, heap, Cursor};

// The host stages params/input/output through the exported `alloc`, which is
// dlmalloc-backed (linear memory grows on demand — no fixed capacity to
// size). The Merkle builder will allocate its per-level hash buffers from
// the same heap once the Poseidon backend is implemented.
#[unsafe(export_name = "alloc")]
pub extern "C" fn alloc(size: u32) -> u32 {
    heap::alloc(size)
}

// Free a staging buffer previously returned by `alloc`. The executor creates
// a fresh instance per run, so nothing calls this yet.
#[unsafe(export_name = "dealloc")]
pub extern "C" fn dealloc(ptr: u32, size: u32) {
    heap::dealloc(ptr, size)
}

pub const ROUND_CONSTANTS: [Fr; 192] = [
    MontFp!("0x1d066a255517b7fd8bddd3a93f7804ef7f8fcde48bb4c37a59a09a1a97052816"),
    MontFp!("0x29daefb55f6f2dc6ac3f089cebcc6120b7c6fef31367b68eb7238547d32c1610"),
    MontFp!("0x1f2cb1624a78ee001ecbd88ad959d7012572d76f08ec5c4f9e8b7ad7b0b4e1d1"),
    MontFp!("0x0aad2e79f15735f2bd77c0ed3d14aa27b11f092a53bbc6e1db0672ded84f31e5"),
    MontFp!("0x2252624f8617738cd6f661dd4094375f37028a98f1dece66091ccf1595b43f28"),
    MontFp!("0x1a24913a928b38485a65a84a291da1ff91c20626524b2b87d49f4f2c9018d735"),
    MontFp!("0x22fc468f1759b74d7bfc427b5f11ebb10a41515ddff497b14fd6dae1508fc47a"),
    MontFp!("0x1059ca787f1f89ed9cd026e9c9ca107ae61956ff0b4121d5efd65515617f6e4d"),
    MontFp!("0x02be9473358461d8f61f3536d877de982123011f0bf6f155a45cbbfae8b981ce"),
    MontFp!("0x0ec96c8e32962d462778a749c82ed623aba9b669ac5b8736a1ff3a441a5084a4"),
    MontFp!("0x292f906e073677405442d9553c45fa3f5a47a7cdb8c99f9648fb2e4d814df57e"),
    MontFp!("0x274982444157b86726c11b9a0f5e39a5cc611160a394ea460c63f0b2ffe5657e"),
    MontFp!("0x1a1d063e54b1e764b63e1855bff015b8cedd192f47308731499573f23597d4b5"),
    MontFp!("0x26abc66f3fdf8e68839d10956259063708235dccc1aa3793b91b002c5b257c37"),
    MontFp!("0x0c7c64a9d887385381a578cfed5aed370754427aabca92a70b3c2b12ff4d7be8"),
    MontFp!("0x1cf5998769e9fab79e17f0b6d08b2d1eba2ebac30dc386b0edd383831354b495"),
    MontFp!("0x0f5e3a8566be31b7564ca60461e9e08b19828764a9669bc17aba0b97e66b0109"),
    MontFp!("0x18df6a9d19ea90d895e60e4db0794a01f359a53a180b7d4b42bf3d7a531c976e"),
    MontFp!("0x04f7bf2c5c0538ac6e4b782c3c6e601ad0ea1d3a3b9d25ef4e324055fa3123dc"),
    MontFp!("0x29c76ce22255206e3c40058523748531e770c0584aa2328ce55d54628b89ebe6"),
    MontFp!("0x198d425a45b78e85c053659ab4347f5d65b1b8e9c6108dbe00e0e945dbc5ff15"),
    MontFp!("0x25ee27ab6296cd5e6af3cc79c598a1daa7ff7f6878b3c49d49d3a9a90c3fdf74"),
    MontFp!("0x138ea8e0af41a1e024561001c0b6eb1505845d7d0c55b1b2c0f88687a96d1381"),
    MontFp!("0x306197fb3fab671ef6e7c2cba2eefd0e42851b5b9811f2ca4013370a01d95687"),
    MontFp!("0x1a0c7d52dc32a4432b66f0b4894d4f1a21db7565e5b4250486419eaf00e8f620"),
    MontFp!("0x2b46b418de80915f3ff86a8e5c8bdfccebfbe5f55163cd6caa52997da2c54a9f"),
    MontFp!("0x12d3e0dc0085873701f8b777b9673af9613a1af5db48e05bfb46e312b5829f64"),
    MontFp!("0x263390cf74dc3a8870f5002ed21d089ffb2bf768230f648dba338a5cb19b3a1f"),
    MontFp!("0x0a14f33a5fe668a60ac884b4ca607ad0f8abb5af40f96f1d7d543db52b003dcd"),
    MontFp!("0x28ead9c586513eab1a5e86509d68b2da27be3a4f01171a1dd847df829bc683b9"),
    MontFp!("0x1c6ab1c328c3c6430972031f1bdb2ac9888f0ea1abe71cffea16cda6e1a7416c"),
    MontFp!("0x1fc7e71bc0b819792b2500239f7f8de04f6decd608cb98a932346015c5b42c94"),
    MontFp!("0x03e107eb3a42b2ece380e0d860298f17c0c1e197c952650ee6dd85b93a0ddaa8"),
    MontFp!("0x2d354a251f381a4669c0d52bf88b772c46452ca57c08697f454505f6941d78cd"),
    MontFp!("0x094af88ab05d94baf687ef14bc566d1c522551d61606eda3d14b4606826f794b"),
    MontFp!("0x19705b783bf3d2dc19bcaeabf02f8ca5e1ab5b6f2e3195a9d52b2d249d1396f7"),
    MontFp!("0x09bf4acc3a8bce3f1fcc33fee54fc5b28723b16b7d740a3e60cef6852271200e"),
    MontFp!("0x1803f8200db6013c50f83c0c8fab62843413732f301f7058543a073f3f3b5e4e"),
    MontFp!("0x0f80afb5046244de30595b160b8d1f38bf6fb02d4454c0add41f7fef2faf3e5c"),
    MontFp!("0x126ee1f8504f15c3d77f0088c1cfc964abcfcf643f4a6fea7dc3f98219529d78"),
    MontFp!("0x23c203d10cfcc60f69bfb3d919552ca10ffb4ee63175ddf8ef86f991d7d0a591"),
    MontFp!("0x2a2ae15d8b143709ec0d09705fa3a6303dec1ee4eec2cf747c5a339f7744fb94"),
    MontFp!("0x07b60dee586ed6ef47e5c381ab6343ecc3d3b3006cb461bbb6b5d89081970b2b"),
    MontFp!("0x27316b559be3edfd885d95c494c1ae3d8a98a320baa7d152132cfe583c9311bd"),
    MontFp!("0x1d5c49ba157c32b8d8937cb2d3f84311ef834cc2a743ed662f5f9af0c0342e76"),
    MontFp!("0x2f8b124e78163b2f332774e0b850b5ec09c01bf6979938f67c24bd5940968488"),
    MontFp!("0x1e6843a5457416b6dc5b7aa09a9ce21b1d4cba6554e51d84665f75260113b3d5"),
    MontFp!("0x11cdf00a35f650c55fca25c9929c8ad9a68daf9ac6a189ab1f5bc79f21641d4b"),
    MontFp!("0x21632de3d3bbc5e42ef36e588158d6d4608b2815c77355b7e82b5b9b7eb560bc"),
    MontFp!("0x0de625758452efbd97b27025fbd245e0255ae48ef2a329e449d7b5c51c18498a"),
    MontFp!("0x2ad253c053e75213e2febfd4d976cc01dd9e1e1c6f0fb6b09b09546ba0838098"),
    MontFp!("0x1d6b169ed63872dc6ec7681ec39b3be93dd49cdd13c813b7d35702e38d60b077"),
    MontFp!("0x1660b740a143664bb9127c4941b67fed0be3ea70a24d5568c3a54e706cfef7fe"),
    MontFp!("0x0065a92d1de81f34114f4ca2deef76e0ceacdddb12cf879096a29f10376ccbfe"),
    MontFp!("0x1f11f065202535987367f823da7d672c353ebe2ccbc4869bcf30d50a5871040d"),
    MontFp!("0x26596f5c5dd5a5d1b437ce7b14a2c3dd3bd1d1a39b6759ba110852d17df0693e"),
    MontFp!("0x16f49bc727e45a2f7bf3056efcf8b6d38539c4163a5f1e706743db15af91860f"),
    MontFp!("0x1abe1deb45b3e3119954175efb331bf4568feaf7ea8b3dc5e1a4e7438dd39e5f"),
    MontFp!("0x0e426ccab66984d1d8993a74ca548b779f5db92aaec5f102020d34aea15fba59"),
    MontFp!("0x0e7c30c2e2e8957f4933bd1942053f1f0071684b902d534fa841924303f6a6c6"),
    MontFp!("0x0812a017ca92cf0a1622708fc7edff1d6166ded6e3528ead4c76e1f31d3fc69d"),
    MontFp!("0x21a5ade3df2bc1b5bba949d1db96040068afe5026edd7a9c2e276b47cf010d54"),
    MontFp!("0x01f3035463816c84ad711bf1a058c6c6bd101945f50e5afe72b1a5233f8749ce"),
    MontFp!("0x0b115572f038c0e2028c2aafc2d06a5e8bf2f9398dbd0fdf4dcaa82b0f0c1c8b"),
    MontFp!("0x1c38ec0b99b62fd4f0ef255543f50d2e27fc24db42bc910a3460613b6ef59e2f"),
    MontFp!("0x1c89c6d9666272e8425c3ff1f4ac737b2f5d314606a297d4b1d0b254d880c53e"),
    MontFp!("0x03326e643580356bf6d44008ae4c042a21ad4880097a5eb38b71e2311bb88f8f"),
    MontFp!("0x268076b0054fb73f67cee9ea0e51e3ad50f27a6434b5dceb5bdde2299910a4c9"),
    MontFp!("0x1acd63c67fbc9ab1626ed93491bda32e5da18ea9d8e4f10178d04aa6f8747ad0"),
    MontFp!("0x19f8a5d670e8ab66c4e3144be58ef6901bf93375e2323ec3ca8c86cd2a28b5a5"),
    MontFp!("0x1c0dc443519ad7a86efa40d2df10a011068193ea51f6c92ae1cfbb5f7b9b6893"),
    MontFp!("0x14b39e7aa4068dbe50fe7190e421dc19fbeab33cb4f6a2c4180e4c3224987d3d"),
    MontFp!("0x1d449b71bd826ec58f28c63ea6c561b7b820fc519f01f021afb1e35e28b0795e"),
    MontFp!("0x1ea2c9a89baaddbb60fa97fe60fe9d8e89de141689d1252276524dc0a9e987fc"),
    MontFp!("0x0478d66d43535a8cb57e9c1c3d6a2bd7591f9a46a0e9c058134d5cefdb3c7ff1"),
    MontFp!("0x19272db71eece6a6f608f3b2717f9cd2662e26ad86c400b21cde5e4a7b00bebe"),
    MontFp!("0x14226537335cab33c749c746f09208abb2dd1bd66a87ef75039be846af134166"),
    MontFp!("0x01fd6af15956294f9dfe38c0d976a088b21c21e4a1c2e823f912f44961f9a9ce"),
    MontFp!("0x18e5abedd626ec307bca190b8b2cab1aaee2e62ed229ba5a5ad8518d4e5f2a57"),
    MontFp!("0x0fc1bbceba0590f5abbdffa6d3b35e3297c021a3a409926d0e2d54dc1c84fda6"),
    MontFp!("0x30347f53e91a637fca1d8a1e828d6fb969e737481ad3376d722513091c0f90c9"),
    MontFp!("0x0de59a358f0ecd2d5bbb3625c3b071a42b475bca9222507c955254e81e2f98b7"),
    MontFp!("0x192367e65f923e2f6ade0fad0239743874b77a8de8088d62cc96f373156ecf16"),
    MontFp!("0x01a992b6af0424b93f830a979873e59685c66affc3a6b6ca87fb421d18dc887d"),
    MontFp!("0x1e9bdf5427a5620701bb81c2f854ad8ee69ff2b4a8069c8869acd5bd3ef74ec8"),
    MontFp!("0x1b256e0fb7d5ec339daa27f20a017a07ba8d4adaf1d05142547f82d6082f7a42"),
    MontFp!("0x2a5bc4ad257499ea42a53a531910f9a32b4db734215d1b8d28256d1b1ef38e70"),
    MontFp!("0x27fcec3b431befcb471c4df705b59ac018f4bb1c58e49c51008f29a51b837f90"),
    MontFp!("0x22961d12dc1f96bce1b57afce557ef947e1b8f20e81273eb5533ef8556278a6c"),
    MontFp!("0x011c5653ac8b64cd159dc124b2dd142fcaeaa2086307c785824d8597f7a1ee1d"),
    MontFp!("0x1d519feae9827d0b1bb7f14a272f5535a35856fdfbff1bf85a059c31d45681df"),
    MontFp!("0x2ee9619acd36e9ec3617767f07407f43d48ba40840a736180bfb2ace24f85c7c"),
    MontFp!("0x2637f99fce7463a906efaadc0c12122e98c670f6363a7e83bf49e225930593db"),
    MontFp!("0x1c12745737824622fd8f15456b011e1d0b9e4526c415ac38892c7bc6de6c5fa8"),
    MontFp!("0x19b98d3fc8e2b487c78fbb1eb365c232cf46a209c7c563163947c8be4d4ee971"),
    MontFp!("0x04bf0ee44e25b5b08c9e5fc181190a5c2548edbbea40951a05e12bf8d3e3fecf"),
    MontFp!("0x1508862a72542035f7da6febb71116e47efc62d62e31404ed3a789216b7f6718"),
    MontFp!("0x29684cede059b92e0d17cd476adc475ca6c001641752ccd3c93483c58e651560"),
    MontFp!("0x11fba1de926dc812f9de635c42f2f4817a5d7203cedf2e1e5b13525d22fae357"),
    MontFp!("0x1c79b44ba583f341aa2cab67a1e377f0b62dc3229bd5951bae6e048d1407695c"),
    MontFp!("0x0efac6637312c7025f8981e3bef465f49d4ed20e9a725159f0e265272d406547"),
    MontFp!("0x0202e9abde9c96289bdae42661a2494d0414b3ca42d99011a4c31ec29a65ed71"),
    MontFp!("0x182965cfa2bd901525ba84ad7540b380eb65d92099c4f2e6bdab55f7bd7a6e36"),
    MontFp!("0x2b228d8943f9f31b13de90198396845ed50cd22e08bb9c7078d84810e5b3fcbc"),
    MontFp!("0x00d577d378751869bdaf4f7a66de23217134dcc67af29ccea82bf5b7f8b53189"),
    MontFp!("0x243b0fa88aedc975cbe2e286dcdc284cac4ab168a524ecc14b1216cf007135b6"),
    MontFp!("0x27c7ca4bf4290d1e6b693322655afe507dbed93efbf39852bb64b331e0e8f39f"),
    MontFp!("0x27d0ab1d52d5dafa31652793025c0b3bc9b1b6d3330a13e05a8432364a9f2b9b"),
    MontFp!("0x14ae1c11de5120e670cf9be3444611983b71e8cdc6b20a2c667632a10a6237bf"),
    MontFp!("0x23d1b30e1e91dc0275a0abaab437389623804c387d91e98054ab0fee62b03e8b"),
    MontFp!("0x2d3071b44b0819a33728c4c945200c5d07b4046697f44a6a9eaa36ec4a768011"),
    MontFp!("0x1c91211710526c8d43588e11dce44e8d19abe4e74255d170e9c17584f0578bdb"),
    MontFp!("0x124d84d94425e4dcc9494762bd423bf08c970c63e9de1a173fe7e877658e3154"),
    MontFp!("0x0a0487e7fe653ff630f59af8443b4f79632e918208bc645fe96e0364711669b8"),
    MontFp!("0x10a8c9fa3ae6b3f010202d63a195e5a1ce6df40b60a158faca9488330a037bb8"),
    MontFp!("0x168dc103f522a4558d97b24a71990ed38203879551ccc4bf3dc57d6043d7821c"),
    MontFp!("0x22417ea97fa7ab926f6b4d36d00a86b03e0f7be7d6d8e2a1955e954c18b33a8b"),
    MontFp!("0x2a6174d4b9fa90538e4539a1bc5d2c88aabfae97ef1e66644d4c4588ddd62c84"),
    MontFp!("0x1cc248057eb0fd28f1f753f5f85fe03ba0ec8f3053b06f4feb4a3dbd496def2f"),
    MontFp!("0x14dbcc08b921c358db26d85746562d0b51917d56eb9e6779664e502bea28462e"),
    MontFp!("0x1d28a4f9cd6146551ebdf33afcb5babfebd3eb9da7857fb0addb965bf3e3a372"),
    MontFp!("0x1596900ce091cea8799615f5f52961461df7f00fca9d73b55574a8085d74b5b3"),
    MontFp!("0x0978d75a71e9ccccc2ff0dbca6a34784e5ca3101c2ea84e1ea7684bbb6e18837"),
    MontFp!("0x1b1f1cb131cb037d14d158726ce96b73e7fa17c075872e056644e73d8d925dd7"),
    MontFp!("0x156eecc345d11b0073e482762012502ee508af74557fb1daac95b01934938e62"),
    MontFp!("0x224421a4d0a2fe503cd90416eb80593e6def8d1f1640804b15df556815548d02"),
    MontFp!("0x0a17879cf1b30bea8c75376232dfa6666a9a106533a677204fe5832cb47e437d"),
    MontFp!("0x25da75173ebcbd286269ed32efbc55ee6db9bb4ebe637705f605c498e663c817"),
    MontFp!("0x0aa00a02a18574063e1186ef3dedb586bbbdc335dcbd30fd8e983b1642929927"),
    MontFp!("0x300e19c48ed4866175f50acdaa379c042c441c1cb34c4001d1fe9358f8b94aad"),
    MontFp!("0x2f22e43e2ec235da7c99e04f7d34d725808e3653d322ac303a5ea1b0c4f6d630"),
    MontFp!("0x03adcd0ed6032a56b61f76a0122c0b67e7c7665ec79da9ee005a50ccf490ea4c"),
    MontFp!("0x235297c114d27b55cbdf5121cf44d611b3e9be47a9c9768f5ab8807fcd2435a7"),
    MontFp!("0x10f1182b447cff3375f3375eff839c2689168f09c65ed44114be7287a2f8b4c8"),
    MontFp!("0x1e6adbf9397247807b6441703ce1a57e61c2ab95de1e7ffba10c4fcf49a57966"),
    MontFp!("0x01a0c48c7936505b63833020c75eb00ea88939a33c810f761a4b045380135456"),
    MontFp!("0x2dbc47b5021936f8c3577fbaa65b4fda57bcebd012ae5e7aa4e77703be6d030e"),
    MontFp!("0x1327666b84984cf65756d28092195e931185a3c928b09d46eb332b35ee5a468c"),
    MontFp!("0x2bc934e3f91921ec3c28edc8c725f79d7e169397ed56d8be18ac39d308636ca9"),
    MontFp!("0x183dd78940fbb6ecd564b267c43b5e5eb87802d66f897aeaf8f03221824a5cc0"),
    MontFp!("0x2c3b99c113caa8215cf5a9377346efac167c54563dd350fac23688aca7fa205b"),
    MontFp!("0x0cfc218f63c5a59e9778251924fcbb0df010313bec2406b7e8ef87a9d82830cd"),
    MontFp!("0x301a1be9217e2cbfa3c9fbb8e1cdba31759539b27c2f4d932b6e075992bc073d"),
    MontFp!("0x0451168db6416d9a2bd56d3b05303d395ecb8636f42c4e2cbcfc996f0a8b8d4f"),
    MontFp!("0x0279fe381976eda48032c8ae75f1aca0662bee641c5df4a96e52da33bd117458"),
    MontFp!("0x2dd3f1dea0c8d9f4793948270d814241747ef420a5f0d75829527d36c740e678"),
    MontFp!("0x1bde2068fd10ccc3eaec0104a0008897fdf255ac8d6694d18d81dbb26682f28b"),
    MontFp!("0x18e9925c649a6bf7c819de04a1a15e1bdff84104178e67c3be59a3213b047613"),
    MontFp!("0x0281fc392973d4972722a9b137a625c903716d7aad74c22795d055cb4323bd14"),
    MontFp!("0x0757134be627b5ff9b3d7a20e7845f384fe26ae2e9c7d161df4e63e8db363415"),
    MontFp!("0x1e96e7da78032be3b45df5375e5aff61db332e1e5c0f67d778d8dee4db8cb576"),
    MontFp!("0x10e29927e946e8145c6f4c615904cbde50fd257d13c87c4bbd0b65b976de377c"),
    MontFp!("0x104f75276d0da2364a0e03d4f115e83167bc3bc3340b86eae7e98192104d6c60"),
    MontFp!("0x01c6368cb969e2f8d255e95d5e962ba969624075cb6dfa5a04b0bd5ed1cd62dc"),
    MontFp!("0x106fffc94ca4acbd764af0e7f76856e1b30a6e067befab8837a5a15cd32be88f"),
    MontFp!("0x15e78bf1f7c8bfe17dbd8a0155728c644fcbc3515aed35dd569b52010f0c95a2"),
    MontFp!("0x000cab14c0ff2cf1718fe666467055d18d8c192e3c02d598a38f5515985d16b8"),
    MontFp!("0x23f34102470d94829f328e6141909b903f45d4deea6ce7ff017803bb1abf9c75"),
    MontFp!("0x1fd2d8ce7613d6b61d65f6ef7284f392e2cf207b0d323ba0ee0fc2aaa6935da6"),
    MontFp!("0x0c63086a8a20a108fa13fc8a57d078f0df03695bad70b3ac02cbdd24374fad45"),
    MontFp!("0x27cd3730e4714199fdb215a5c8d967f46b185fcc2f0e548eaab631aae5a2b54c"),
    MontFp!("0x15adaa75fc1f1595186c0d4d4a0164604cd1a1cf69f15fca5ec79c1e524be22b"),
    MontFp!("0x05aa5e4fb84931226fe71314cdc4d64bd1e15619a346a9e8853183fb7ae19d02"),
    MontFp!("0x27fb8cd694fcd1d058313959fcfc621a3ccb7a9a8ea245ac31c13fb6a57c4022"),
    MontFp!("0x2be0953fd8b1d2f6e463ee9a3f70e3e817565d61af26c621713b207ad34ec7a7"),
    MontFp!("0x217143e8ae458a9ef116ca2a15fc36cc469fcbe87b9c1ab6fce03b25d20c25a2"),
    MontFp!("0x29c3b69f65b5cfd2cffd3123a0118d90f945e1eba04fbcd916fa11b3d59328f2"),
    MontFp!("0x2951ccd20b0a35b9603de573d11918a98e99662b3311144ef81c16c84ed32fe9"),
    MontFp!("0x202d7cf41dcbbb10b69b64f3e7d609617b9ee088de1db8b7ac4bcfba87dbd048"),
    MontFp!("0x014d390c7229d74a5b39dddc6f0395eec036a1e0d377cfcc9b1ad0686c3743b5"),
    MontFp!("0x1479c1cfbd48817240820dc11e59d9bf16f7dfa3ebfcc3d4dd7e97292296262e"),
    MontFp!("0x0684d98bb96761750f65d8933ab43397d4a96d8b6c61ded5816fcd74b562dfa0"),
    MontFp!("0x1f4f4cd32539eddcda05a729297a2a5f892cd50179df7ce1e4008373c89447a8"),
    MontFp!("0x03326d7fdcd6ccc2371731b5752d957d1dfd792a5351c10bd958ffb04635b84f"),
    MontFp!("0x1d5b99cb1e95e9d975bd7f99d1a95d7f0a5688d7e4965aafbd76a271cb6e876d"),
    MontFp!("0x13d909a621a86fcb4e9978dae7f77a014174cc9f6ff6b8ba496f0513d2af1054"),
    MontFp!("0x16e7671d2d3a50c7cdbf3270e8bf1c4f18221f7bd7f899e313c45e28174d17ce"),
    MontFp!("0x03aac5e52aedb6acad82466f062d38d8294c6af6b5900c700c5323684153fdcd"),
    MontFp!("0x086f0806c45cf713dc2c19a3332faba6df3a4c8c93e292c996d0886c81916d34"),
    MontFp!("0x2a845e4cb08384e51a40a14687da5aa91f9807d575b0d88a34e1f85a2e20e969"),
    MontFp!("0x18d2a59257afc8bd005f3b2804cbeea1cab18b6a62efe2f4742d1e0eccf06e8b"),
    MontFp!("0x1a2d3094ec6931ac4d53e69338e5fb98610dea28731816029b8ad15be5361be9"),
    MontFp!("0x1cfe7a330a5001825299978e53f555dcd300210260c1d589aeca0df6089cad6d"),
    MontFp!("0x0da40fff9f10c73aea59002d40230b8933f2fdbc192553057e2784530825b921"),
    MontFp!("0x0e05b77a1a396b75dbf6e8e234f30c846561de1faec70eefc0393df01e079ff5"),
    MontFp!("0x1a044b846a4bb239dcd58b95d6656a452151bec40f3084b4325f918029bf262e"),
    MontFp!("0x2e139ae51418b64f78043335ddcab1b8c349540db935e2bca89493b0ea189784"),
    MontFp!("0x0741808912ca9cbf94a0228663125866e4f28f77af515b1170ee063a7f676240"),
    MontFp!("0x0b29628ee57e1d55f70f9059bb80e3608642e046003218807f69b40d94e8cc91"),
    MontFp!("0x060804c31fb3be30dd5475cdcabce8afa21ddf46b4631583565f949933fbf9d1"),
    MontFp!("0x2760f6b6590a73a863f9bd30d8cb6002d195fa252b0cd84f36d3955e853b3592"),
    MontFp!("0x14aa7543a56c144a6fa53cb6d2e0537c2c65b571104def5a67d6f228492c2c5b"),
];
 pub const MDS_MATRIX: [[Fr; 3]; 3] = [
    [MontFp!("0x08839af91040661d88438f506cfb1158bad2c1194ed8d9edc6daffddceb03640"), MontFp!("0x2b4818de7150d505bc7419a2e9174fc3308780492843ad0e4282f67754712c5b"), MontFp!("0x29fde99b8b86ab7c5c26bf594b23378ffcf1d58e06a5380adca08a2c7a922856")],
    [MontFp!("0x05e80b8117ef7990abad0f1c04c0d2672b8ef2c5782ccfb515fdb140943a4191"), MontFp!("0x1612fe911fa09cf282de92a7470faf266839c0a05f99525e952cc7105b054b6d"), MontFp!("0x19794c9c98d0c696c10abe4b3e6755891f92218ca1deb0d293ac27bc793f6b6b")],
    [MontFp!("0x28cbc5ccb9468e72c9a93e39438cd0d2101d2720c3d796d4a5e25e3e986f178b"), MontFp!("0x0d50774240a9f560ee85bc3cbacb200de7bbe1dbee378dcf9388d677efb69b74"), MontFp!("0x18092553cfdffa71625c676e0af6f3bd8e0107fa4b7c5e459f40432c525c5ba5")]
];

fn pkcs7_pad(data: &[u8]) -> [u8; 31] {
    let pad_len = (31 - data.len()) as u8; // always in 1..=31
    let mut out = [0u8; 31];
    out[..data.len()].copy_from_slice(data);
    out[data.len()..].fill(pad_len);
    return out;
}
extern "C" fn poseidon_one_pass(
    data_ptr: u32,
    data_len: u32,
    output_ptr: u32
) -> Result<(), ()> {
    if data_len != 96 {
        return Err(());
    }
    let mut internal_state = [Zero::zero(); 3];
    unsafe {
        for index in 0..3 {
            let val_start_ptr: *const u8 = (data_ptr + (index * 32) as u32) as *const u8;
            let chunk: &[u8] = core::slice::from_raw_parts(val_start_ptr, 31);
            let elt = Fr::from_be_bytes_mod_order(chunk);
            internal_state[index] = elt;
        }
    }

    let r_f: u8 = 8;
    let r_p: u8 = 56;
    let round_constant_index: usize = 0;

    for _ in 0..r_f / 2 {
        // Add round constants & s-box
        for indx in 0..3 {
            internal_state[indx] += ROUND_CONSTANTS[round_constant_index];
            internal_state[indx] = internal_state[indx].pow([5u64]);
        }
        // Mix layer
        let ele0 = internal_state[0] * MDS_MATRIX[0][0]
            + internal_state[1] * MDS_MATRIX[1][0]
            + internal_state[2] * MDS_MATRIX[2][0];
        let ele1 = internal_state[0] * MDS_MATRIX[0][1]
            + internal_state[1] * MDS_MATRIX[1][1]
            + internal_state[2] * MDS_MATRIX[2][1];
        let ele2 = internal_state[0] * MDS_MATRIX[0][2]
            + internal_state[1] * MDS_MATRIX[1][2]
            + internal_state[2] * MDS_MATRIX[2][2];
        internal_state[0] = ele0;
        internal_state[1] = ele1;
        internal_state[2] = ele2;
    }

    for _ in 0..r_p {
        // Add round constants & s-box
        for indx in 0..3 {
            internal_state[indx] += ROUND_CONSTANTS[round_constant_index];
        }
        internal_state[2] = internal_state[2].pow([5u64]);
        // Mix layer
        let ele0 = internal_state[0] * MDS_MATRIX[0][0]
            + internal_state[1] * MDS_MATRIX[1][0]
            + internal_state[2] * MDS_MATRIX[2][0];
        let ele1 = internal_state[0] * MDS_MATRIX[0][1]
            + internal_state[1] * MDS_MATRIX[1][1]
            + internal_state[2] * MDS_MATRIX[2][1];
        let ele2 = internal_state[0] * MDS_MATRIX[0][2]
            + internal_state[1] * MDS_MATRIX[1][2]
            + internal_state[2] * MDS_MATRIX[2][2];
        internal_state[0] = ele0;
        internal_state[1] = ele1;
        internal_state[2] = ele2;
    }

    for _ in 0..r_f / 2 {
        // Add round constants & s-box
        for indx in 0..3 {
            internal_state[indx] += ROUND_CONSTANTS[round_constant_index];
            internal_state[indx] = internal_state[indx].pow([5u64]);
        }
        // Mix layer
        let ele0 = internal_state[0] * MDS_MATRIX[0][0]
            + internal_state[1] * MDS_MATRIX[1][0]
            + internal_state[2] * MDS_MATRIX[2][0];
        let ele1 = internal_state[0] * MDS_MATRIX[0][1]
            + internal_state[1] * MDS_MATRIX[1][1]
            + internal_state[2] * MDS_MATRIX[2][1];
        let ele2 = internal_state[0] * MDS_MATRIX[0][2]
            + internal_state[1] * MDS_MATRIX[1][2]
            + internal_state[2] * MDS_MATRIX[2][2];
        internal_state[0] = ele0;
        internal_state[1] = ele1;
        internal_state[2] = ele2;
    }

    unsafe {
        let mut block = core::slice::from_raw_parts_mut(output_ptr as *mut u8, 96);
        let ele0 = BigInt::from(internal_state[0]);
        let ele1 = BigInt::from(internal_state[1]);
        let ele2 = BigInt::from(internal_state[2]);
        block[0..32].copy_from_slice(&ele0.to_bytes_be());
        block[32..64].copy_from_slice(&ele1.to_bytes_be());
        block[64..96].copy_from_slice(&ele2.to_bytes_be());
    }

    return Ok(());
}


#[unsafe(export_name = "run")]
// _input_ptr/_input_len/_out_ptr/_out_len_ptr: the uniform 6-arg ABI is kept
// for host dispatch, but only the params are consumed by this stage.
pub extern "C" fn run(
    input_ptr: u32,
    input_len: u32,
    params_ptr: u32,
    params_len: u32,
    out_ptr: u32,
    _out_len_ptr: u32,
) -> i32 {
    // ---- Parameter parsing (Spec.md §6, 5 bytes total) ----
    let mut param_cursor = Cursor::new(params_ptr, params_len);

    // version: u8 at offset 0. Mismatch is always rejected, never coerced.
    let version: u8 = match param_cursor.read_bytes(1) {
        Ok(v) => v[0],
        Err(()) => return E_INVALID_PARAM,
    };
    if version != 0x01 {
        return E_INVALID_PARAM;
    }
    // Exactly 1 bytes remain after the version byte; trailing bytes are
    // always rejected (Spec.md §0).
    let remaining = param_cursor.remaining();
    if remaining > 1 {
        return E_TRAILING_DATA;
    }
    if remaining < 1 {
        return E_INVALID_PARAM;
    }

    // Do we have to create a Merkle tree, or do we just have to create
    // a checksum for the input blob? 1 byte, 0 or 1.
    let is_merkle: u8 = match param_cursor.read_bytes(1) {
        Ok(v) => v[0],
        Err(()) => return E_INVALID_PARAM,
    };
    if is_merkle != 0 && is_merkle != 1 {
        return E_INVALID_PARAM;
    }

    // ---- Digest / Merkleization (Spec.md §6, steps 1-3) ----
    if is_merkle == 0 {
        let hash_result_ptr: u32;
        unsafe {
            hash_result_ptr = alloc(96);
        }
        let num_full_block = input_len / 31;
        let padding: u32 = input_len % 31;
        
        for indx in 0..num_full_block {
            unsafe {
                let current_block_ptr = input_ptr + (indx * 31);
                let mut hashing_mem = core::slice::from_raw_parts_mut(hash_result_ptr as *mut u8, 96);
                let block = core::slice::from_raw_parts(current_block_ptr as *const u8, 31);
                
                let current = Fr::from_be_bytes_mod_order(&hashing_mem[0..32]);
                let incoming = Fr::from_be_bytes_mod_order(block);
                let sum = current + incoming;
                let sum_bigint = BigInt::from(sum);
                hashing_mem[0..32].copy_from_slice(&sum_bigint.to_bytes_be());
                match poseidon_one_pass(hash_result_ptr, 96, hash_result_ptr) {
                    Ok(()) => {}
                    Err(()) => return E_INVALID_PARAM,
                }
            }
        }
        unsafe {
            // The current block might need some padding.
            // Pad it using pkcs7_pad, then hash it like the code above.
            let block_ptr = input_ptr + (num_full_block * 31);
            if padding > 0 {
                // Spec.md §6 step 2: pad a short final chunk with PKCS#7,
                // then absorb it exactly like the full-block loop above.
                let block_slice = core::slice::from_raw_parts(block_ptr as *const u8, padding as usize);
                let padded: [u8; 31] = pkcs7_pad(block_slice);
                let mut hashing_mem = core::slice::from_raw_parts_mut(hash_result_ptr as *mut u8, 96);
                let current = Fr::from_be_bytes_mod_order(&hashing_mem[0..32]);
                let incoming = Fr::from_be_bytes_mod_order(&padded);
                let sum = current + incoming;
                let sum_bigint = BigInt::from(sum);
                hashing_mem[0..32].copy_from_slice(&sum_bigint.to_bytes_be());
                match poseidon_one_pass(hash_result_ptr, 96, hash_result_ptr) {
                    Ok(()) => {}
                    Err(()) => return E_INVALID_PARAM,
                }
            }
        }
        
        unsafe {
            let output_slice = core::slice::from_raw_parts_mut(out_ptr as *mut u8, 32);
            let hash_slice = core::slice::from_raw_parts(hash_result_ptr as *const u8, 32);
            output_slice.copy_from_slice(&hash_slice);
        } 
    }
    else {
       // ---- Merkleization (Spec.md §6, steps 1, 3-4) ----
        let num_full_block = input_len / 31;
        let padding: u32 = input_len % 31;
        let num_chunks = if padding > 0 { num_full_block + 1 } else { num_full_block };

        // Not specified by Spec.md what an empty blob's root should be —
        // rejecting for now rather than guessing a convention.
        if num_chunks == 0 {
            return E_MALFORMED_INPUT;
        }
    
        // Step 1: decode every 31-byte chunk into a field element. Last chunk
        // is PKCS#7-padded if short. This is the bottom level of the tree.
        let mut level: Vec<Fr> = Vec::with_capacity(num_chunks as usize);
        for indx in 0..num_full_block {
            unsafe {
                let block_ptr = input_ptr + (indx * 31);
                let block = core::slice::from_raw_parts(block_ptr as *const u8, 31);
                level.push(Fr::from_be_bytes_mod_order(block));
            }
        }
        if padding > 0 {
            unsafe {
                let block_ptr = input_ptr + (num_full_block * 31);
                let block_slice = core::slice::from_raw_parts(block_ptr as *const u8, padding as usize);
                let padded: [u8; 31] = pkcs7_pad(block_slice);
                level.push(Fr::from_be_bytes_mod_order(&padded));
            }
        }
    
        // Steps 3-4: fold the level upward. At every level — chunk level and
        // internal levels alike — a leftover odd node is promoted unchanged
        // to the next level rather than hashed or zero-padded.
        let hash_buf_ptr: u32 = alloc(96);
    
        while level.len() > 1 {
            let mut next_level: Vec<Fr> = Vec::with_capacity((level.len() + 1) / 2);
            let mut indx = 0usize;
            while indx + 1 < level.len() {
                let left = level[indx];
                let right = level[indx + 1];
                // State layout mirrors the checksum sponge: index 0 is the
                // capacity/output slot (zeroed before each compression),
                // indices 1-2 are the rate, carrying (left, right) in.
                let parent = unsafe {
                    let state = core::slice::from_raw_parts_mut(hash_buf_ptr as *mut u8, 96);
                    state[0..32].fill(0);
                    state[32..64].copy_from_slice(&BigInt::from(left).to_bytes_be());
                    state[64..96].copy_from_slice(&BigInt::from(right).to_bytes_be());
                    match poseidon_one_pass(hash_buf_ptr, 96, hash_buf_ptr) {
                        Ok(()) => {}
                        Err(()) => return E_INVALID_PARAM,
                    }
                    Fr::from_be_bytes_mod_order(&state[0..32])
                };
                next_level.push(parent);
                indx += 2;
            }
            if indx < level.len() {
                // Odd leftover node: promoted as-is, no hashing.
                next_level.push(level[indx]);
            }
            level = next_level;
        }

        unsafe {
            let output_slice = core::slice::from_raw_parts_mut(out_ptr as *mut u8, 32);
            let root_bigint = BigInt::from(level[0]);
            output_slice.copy_from_slice(&root_bigint.to_bytes_be());
        }      
    }
    return E_OK;
}
