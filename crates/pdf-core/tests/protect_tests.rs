use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream};
use pdf_core::operations::{op_compress, op_protect, op_unlock};
use pdf_core::OperationRequest;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// RAII temporary directory fixture that automatically cleans up all written test files on Drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("zp-protect-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("Failed to create temporary directory");
        Self { path }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn sample(path: &Path) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf 20 100 Td (gizli metin) Tj ET".to_vec()));
    let page = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages_id,"MediaBox"=>vec![0.into(),0.into(),200.into(),200.into()],"Contents"=>content,"Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}}});
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}));
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    doc.trailer.set("Root", catalog);
    doc.save(path).unwrap();
}

fn request(kind: &str, input: &str, output: &str, password: Option<&str>, new_password: Option<&str>) -> OperationRequest {
    serde_json::from_value(serde_json::json!({
        "id":"t","kind":kind,"inputs":[input],"output":output,
        "password":password,"newPassword":new_password
    })).unwrap()
}

fn make_request(
    kind: &str,
    input: &str,
    output: &str,
    password: Option<&str>,
    new_password: Option<&str>,
    input_password: Option<&str>,
) -> OperationRequest {
    let mut request = request(kind, input, output, password, new_password);
    if let Some(password) = input_password {
        request.input_passwords.insert(input.into(), password.into());
    }
    request
}


/// Builds a rich PDF document containing real text, Info dictionary, XMP metadata stream,
/// AcroForm text field, and Catalog Outlines with valid page Destination.
/// Encrypts with AES-256 (V5, R6) using fresh file key and file ID with distinct user/owner passwords.
/// If `direct_encrypt` is true, removes the indirect Encrypt object and places the Encrypt dictionary
/// directly inline in the trailer before saving.
fn build_comprehensive_fixture(
    path: &Path,
    user_password: &str,
    owner_password: &str,
    direct_encrypt: bool,
) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    // Real text stream
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 20 100 Td (Zoria Secret Plaintext 2026) Tj ET".to_vec(),
    ));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
        "Contents" => content,
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => font },
        },
    });

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );

    // XMP Metadata stream: /Type /Metadata /Subtype /XML
    let xmp_bytes = b"<?xpacket begin='' id='W5M0MpCehiHzreSzNTczkc9d'?><x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><rdf:Description rdf:about='' xmlns:dc='http://purl.org/dc/elements/1.1/'><dc:title><rdf:Alt><rdf:li xml:lang='x-default'>Zoria Protected Doc</rdf:li></rdf:Alt></dc:title></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end='w'?>".to_vec();
    let metadata_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "Metadata",
            "Subtype" => "XML",
        },
        xmp_bytes,
    ));

    // AcroForm with text field
    let field_id = doc.add_object(dictionary! {
        "FT" => "Tx",
        "T" => Object::string_literal("customer_name"),
        "V" => Object::string_literal("SecretFieldValue42"),
    });
    let acroform_id = doc.add_object(dictionary! {
        "Fields" => vec![field_id.into()],
    });

    // Catalog Outlines: Outline item with valid page Destination [page_id, /Fit]
    let outlines_id = doc.new_object_id();
    let outline_item_id = doc.new_object_id();
    doc.objects.insert(
        outline_item_id,
        Object::Dictionary(dictionary! {
            "Title" => Object::string_literal("Chapter 1: Zoria Outline"),
            "Parent" => outlines_id,
            "Dest" => vec![page_id.into(), Object::Name(b"Fit".to_vec())],
        }),
    );
    doc.objects.insert(
        outlines_id,
        Object::Dictionary(dictionary! {
            "Type" => "Outlines",
            "First" => outline_item_id,
            "Last" => outline_item_id,
            "Count" => 1,
        }),
    );

    // Catalog Root
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "Metadata" => metadata_id,
        "AcroForm" => acroform_id,
        "Outlines" => outlines_id,
    });
    doc.trailer.set("Root", catalog_id);

    // Info Title
    let info_id = doc.add_object(dictionary! {
        "Title" => Object::string_literal("Zoria Encrypted Info Title"),
        "Author" => Object::string_literal("Zoria Author"),
    });
    doc.trailer.set("Info", info_id);

    // Random File ID in trailer
    pdf_core::operations::protect::ensure_file_id(&mut doc).unwrap();

    // R6 V5 AES-256 encryption state with fresh 32-byte key
    let mut file_key = [0u8; 32];
    getrandom::fill(&mut file_key).unwrap();
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let version = EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        file_encryption_key: &file_key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password,
        user_password,
        permissions: Permissions::all(),
    };
    let state = EncryptionState::try_from(version).unwrap();
    doc.encrypt(&state).unwrap();

    // Direct Encrypt variant: move Encrypt dictionary inline into trailer and remove indirect object
    if direct_encrypt {
        let id = doc.trailer.get(b"Encrypt").unwrap().as_reference().unwrap();
        let dictionary = doc.objects.remove(&id).unwrap().as_dict().unwrap().clone();
        doc.trailer.set("Encrypt", Object::Dictionary(dictionary));
    }

    doc.save(path).unwrap();
}

/// Verifies that a decrypted/unlocked/compressed document is plain and has preserved all
/// required structural, text, metadata, form, and outline elements.
fn assert_plain_document_intact(doc: &Document) {
    assert!(!doc.trailer.has(b"Encrypt"));
    let pages = doc.get_pages();
    assert_eq!(pages.len(), 1);
    let &page = pages.values().next().unwrap();
    let content = doc.get_page_content(page);
    assert!(String::from_utf8_lossy(&content).contains("Zoria Secret Plaintext 2026"));
    let info = doc.get_dict_in_dict(&doc.trailer, b"Info").unwrap();
    assert_eq!(info.get(b"Title").unwrap().as_str().unwrap(), b"Zoria Encrypted Info Title");
    let root = doc.get_dict_in_dict(&doc.trailer, b"Root").unwrap();
    let metadata = doc.get_object(root.get(b"Metadata").unwrap().as_reference().unwrap()).unwrap().as_stream().unwrap();
    let xml = metadata.decompressed_content().unwrap_or_else(|_| metadata.content.clone());
    assert!(String::from_utf8_lossy(&xml).contains("Zoria Protected Doc"));
    let acroform = doc.get_dict_in_dict(root, b"AcroForm").unwrap();
    let fields = acroform.get(b"Fields").unwrap().as_array().unwrap();
    assert_eq!(fields.len(), 1);
    let field = doc.get_object(fields[0].as_reference().unwrap()).unwrap().as_dict().unwrap();
    assert_eq!(field.get(b"T").unwrap().as_str().unwrap(), b"customer_name");
    assert_eq!(field.get(b"V").unwrap().as_str().unwrap(), b"SecretFieldValue42");
    let outlines = doc.get_dict_in_dict(root, b"Outlines").unwrap();
    let item = doc.get_object(outlines.get(b"First").unwrap().as_reference().unwrap()).unwrap().as_dict().unwrap();
    assert_eq!(item.get(b"Title").unwrap().as_str().unwrap(), b"Chapter 1: Zoria Outline");
    let destination = item.get(b"Dest").unwrap().as_array().unwrap();
    assert_eq!(destination[0].as_reference().unwrap(), page);
    assert_eq!(destination[1].as_name().unwrap(), b"Fit");
}

#[test]
fn protect_writes_aes256_r6_and_unlock_round_trips() {
    let temp = TempDir::new();
    let input = temp.file("in.pdf");
    sample(&input);
    let locked = input.with_file_name("locked.pdf");
    let cancel = AtomicBool::new(false);
    op_protect(&request("protect", input.to_str().unwrap(), locked.to_str().unwrap(), None, Some("şifreİ")), &cancel, |_| {}).unwrap();

    let raw = Document::load(&locked).expect("encrypted file still parses");
    let encrypt = raw.get_encrypted().expect("Encrypt dictionary present");
    assert_eq!(encrypt.get(b"V").unwrap().as_i64().unwrap(), 5);
    assert_eq!(encrypt.get(b"R").unwrap().as_i64().unwrap(), 6);
    assert!(Document::load_with_password(&locked, "wrong").is_err());

    let unlocked = input.with_file_name("unlocked.pdf");
    op_unlock(&request("unlock", locked.to_str().unwrap(), unlocked.to_str().unwrap(), Some("şifreİ"), None), &cancel, |_| {}).unwrap();
    let plain = Document::load(&unlocked).unwrap();
    assert!(plain.get_encrypted().is_err());
    assert_eq!(plain.get_pages().len(), 1);
}

#[test]
fn unlock_still_opens_legacy_rc4_files() {
    let temp = TempDir::new();
    let input = temp.file("in.pdf");
    sample(&input);
    let mut doc = Document::load(&input).unwrap();
    pdf_core::operations::protect::ensure_file_id(&mut doc).unwrap();
    let state = EncryptionState::try_from(EncryptionVersion::V2 {
        document: &doc, owner_password: "eski", user_password: "eski", key_length: 128,
        permissions: Permissions::all(),
    }).unwrap();
    doc.encrypt(&state).unwrap();
    let legacy = input.with_file_name("rc4.pdf");
    doc.save(&legacy).unwrap();
    let out = input.with_file_name("open.pdf");
    op_unlock(&request("unlock", legacy.to_str().unwrap(), out.to_str().unwrap(), Some("eski"), None), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(Document::load(&out).unwrap().get_encrypted().is_err());
}

#[test]
fn test_direct_and_indirect_encrypt_unlock_user_and_owner_passwords() {
    for direct in [false, true] {
        let temp = TempDir::new();
        let input = temp.file(if direct { "direct.pdf" } else { "indirect.pdf" });
        let user_pw = "user_secret_r6_pass";
        let owner_pw = "owner_secret_r6_pass";

        build_comprehensive_fixture(&input, user_pw, owner_pw, direct);

        // Verify encrypted file representation
        let raw = Document::load(&input).expect("Encrypted file parses");
        let encrypt = raw.get_encrypted().expect("Encrypt dictionary present");
        assert_eq!(encrypt.get(b"V").unwrap().as_i64().unwrap(), 5);
        assert_eq!(encrypt.get(b"R").unwrap().as_i64().unwrap(), 6);
        if direct {
            assert!(
                matches!(raw.trailer.get(b"Encrypt").unwrap(), Object::Dictionary(_)),
                "Direct variant must store Encrypt dictionary inline in trailer"
            );
        } else {
            assert!(
                matches!(raw.trailer.get(b"Encrypt").unwrap(), Object::Reference(_)),
                "Indirect variant must store Encrypt as an indirect reference"
            );
        }

        let cancel = AtomicBool::new(false);

        // 1. Unlock with USER password
        let unlocked_user = temp.file(if direct { "unlocked_direct_user.pdf" } else { "unlocked_indirect_user.pdf" });
        let req_user = make_request(
            "unlock",
            input.to_str().unwrap(),
            unlocked_user.to_str().unwrap(),
            Some(user_pw),
            None,
            None,
        );
        op_unlock(&req_user, &cancel, |_| {}).expect("Unlock with USER password must succeed");
        assert!(unlocked_user.exists(), "Unlocked output file must exist");
        let plain_user = Document::load(&unlocked_user).expect("Unlocked file parses as PDF");
        assert_plain_document_intact(&plain_user);

        // 2. Unlock with OWNER password via input_passwords map
        let unlocked_owner = temp.file(if direct { "unlocked_direct_owner.pdf" } else { "unlocked_indirect_owner.pdf" });
        let req_owner = make_request(
            "unlock",
            input.to_str().unwrap(),
            unlocked_owner.to_str().unwrap(),
            None,
            None,
            Some(owner_pw),
        );
        op_unlock(&req_owner, &cancel, |_| {}).expect("Unlock with OWNER password must succeed");
        assert!(unlocked_owner.exists(), "Unlocked output file must exist");
        let plain_owner = Document::load(&unlocked_owner).expect("Unlocked file parses as PDF");
        assert_plain_document_intact(&plain_owner);
    }
}

#[test]
fn test_direct_and_indirect_encrypt_compress() {
    for direct in [false, true] {
        let temp = TempDir::new();
        let input = temp.file(if direct { "direct_comp.pdf" } else { "indirect_comp.pdf" });
        let user_pw = "user_comp_pass_456";
        let owner_pw = "owner_comp_pass_789";

        build_comprehensive_fixture(&input, user_pw, owner_pw, direct);
        let cancel = AtomicBool::new(false);

        // 1. Compress with USER password
        let compressed_user = temp.file(if direct { "compressed_direct_user.pdf" } else { "compressed_indirect_user.pdf" });
        let req_user = make_request(
            "compress",
            input.to_str().unwrap(),
            compressed_user.to_str().unwrap(),
            Some(user_pw),
            None,
            None,
        );
        op_compress(&req_user, &cancel, |_| {}).expect("Compress with USER password must succeed");
        assert!(compressed_user.exists(), "Compressed output file must exist");
        let plain_user = Document::load(&compressed_user).expect("Compressed file parses");
        assert_plain_document_intact(&plain_user);

        // 2. Compress with OWNER password via input_passwords
        let compressed_owner = temp.file(if direct { "compressed_direct_owner.pdf" } else { "compressed_indirect_owner.pdf" });
        let req_owner = make_request(
            "compress",
            input.to_str().unwrap(),
            compressed_owner.to_str().unwrap(),
            None,
            None,
            Some(owner_pw),
        );
        op_compress(&req_owner, &cancel, |_| {}).expect("Compress with OWNER password must succeed");
        assert!(compressed_owner.exists(), "Compressed output file must exist");
        let plain_owner = Document::load(&compressed_owner).expect("Compressed file parses");
        assert_plain_document_intact(&plain_owner);
    }
}

#[test]
fn test_direct_and_indirect_encrypt_reprotect() {
    for direct in [false, true] {
        let temp = TempDir::new();
        let input = temp.file(if direct { "direct_reprot.pdf" } else { "indirect_reprot.pdf" });
        let old_user_pw = "old_user_pass_111";
        let old_owner_pw = "old_owner_pass_222";
        let new_pw = "new_protected_pass_333";

        build_comprehensive_fixture(&input, old_user_pw, old_owner_pw, direct);
        let cancel = AtomicBool::new(false);

        // Re-protect using old user password and specify new password
        let reprotected = temp.file(if direct { "reprotected_direct.pdf" } else { "reprotected_indirect.pdf" });
        let req = make_request(
            "protect",
            input.to_str().unwrap(),
            reprotected.to_str().unwrap(),
            Some(old_user_pw),
            Some(new_pw),
            None,
        );
        op_protect(&req, &cancel, |_| {}).expect("Re-protecting with valid old password must succeed");
        assert!(reprotected.exists(), "Re-protected file must exist");

        // Verify old password and wrong password no longer open the file
        assert!(
            Document::load_with_password(&reprotected, "wrong_pw").is_err(),
            "Wrong password must fail on re-protected PDF"
        );
        assert!(
            Document::load_with_password(&reprotected, old_user_pw).is_err(),
            "Old password must no longer open re-protected PDF"
        );

        // Unlock re-protected PDF with new password and verify plain contents intact
        let unlocked = temp.file(if direct { "final_unlocked_direct.pdf" } else { "final_unlocked_indirect.pdf" });
        let unlock_req = make_request(
            "unlock",
            reprotected.to_str().unwrap(),
            unlocked.to_str().unwrap(),
            Some(new_pw),
            None,
            None,
        );
        op_unlock(&unlock_req, &cancel, |_| {}).expect("Unlock with new password must succeed");
        let plain = Document::load(&unlocked).expect("Final unlocked file parses");
        assert_plain_document_intact(&plain);

        // Also verify re-protecting using old OWNER password via input_passwords
        let reprotected_owner = temp.file(if direct { "reprot_owner_direct.pdf" } else { "reprot_owner_indirect.pdf" });
        let owner_new_pw = "brand_new_owner_set_444";
        let req_owner = make_request(
            "protect",
            input.to_str().unwrap(),
            reprotected_owner.to_str().unwrap(),
            None,
            Some(owner_new_pw),
            Some(old_owner_pw),
        );
        op_protect(&req_owner, &cancel, |_| {}).expect("Re-protecting with old owner password must succeed");
        assert!(Document::load_with_password(&reprotected_owner, old_owner_pw).is_err());
        let mut plain = Document::load_with_password(&reprotected_owner, owner_new_pw).unwrap();
        if plain.trailer.has(b"Encrypt") { plain.decrypt(owner_new_pw).unwrap(); }
        assert_plain_document_intact(&plain);
    }
}

#[test]
fn test_direct_and_indirect_encrypt_invalid_or_missing_password_fails_without_output() {
    for direct in [false, true] {
        let temp = TempDir::new();
        let input = temp.file(if direct { "direct_fail.pdf" } else { "indirect_fail.pdf" });
        let user_pw = "correct_user_pw";
        let owner_pw = "correct_owner_pw";

        build_comprehensive_fixture(&input, user_pw, owner_pw, direct);
        let cancel = AtomicBool::new(false);

        for kind in ["unlock", "compress", "protect"] {
            for (index, password) in [None, Some(""), Some("wrong_password")].into_iter().enumerate() {
                let output = temp.file(&format!("{kind}-{index}.pdf"));
                let request = make_request(kind, input.to_str().unwrap(), output.to_str().unwrap(),
                    password, Some("new-password"), None);
                let result = match kind {
                    "unlock" => op_unlock(&request, &cancel, |_| {}),
                    "compress" => op_compress(&request, &cancel, |_| {}),
                    "protect" => op_protect(&request, &cancel, |_| {}),
                    _ => unreachable!(),
                };
                assert!(result.is_err(), "{kind} accepted an invalid or absent source password");
                assert!(!output.exists(), "{kind} created output after rejecting its source password");
            }
        }
    }
}

#[test]
fn test_direct_and_indirect_empty_user_password_pdf() {
    for direct in [false, true] {
        let temp = TempDir::new();
        let input = temp.file(if direct { "direct_empty_user.pdf" } else { "indirect_empty_user.pdf" });
        let user_pw = "";
        let owner_pw = "owner_strong_secret_999";

        build_comprehensive_fixture(&input, user_pw, owner_pw, direct);

        let cancel = AtomicBool::new(false);

        // 1. op_unlock with explicit empty password ("") succeeds without double-decrypt error
        let unlocked = temp.file(if direct { "unlocked_empty_direct.pdf" } else { "unlocked_empty_indirect.pdf" });
        let req_unlock = make_request(
            "unlock",
            input.to_str().unwrap(),
            unlocked.to_str().unwrap(),
            Some(""),
            None,
            None,
        );
        op_unlock(&req_unlock, &cancel, |_| {}).expect("Unlock on empty user password PDF must succeed");
        assert!(unlocked.exists(), "Unlocked file must exist");
        let plain = Document::load(&unlocked).expect("Unlocked file parses");
        assert_plain_document_intact(&plain);

        // 2. op_compress with omitted password (None, defaulting to empty) succeeds
        let compressed = temp.file(if direct { "compressed_empty_direct.pdf" } else { "compressed_empty_indirect.pdf" });
        let req_compress = make_request(
            "compress",
            input.to_str().unwrap(),
            compressed.to_str().unwrap(),
            None,
            None,
            None,
        );
        op_compress(&req_compress, &cancel, |_| {}).expect("Compress on empty user password PDF must succeed");
        assert!(compressed.exists(), "Compressed file must exist");
        let plain_comp = Document::load(&compressed).expect("Compressed file parses");
        assert_plain_document_intact(&plain_comp);

        // 3. op_protect on empty user password PDF to set a new password
        let reprotected = temp.file(if direct { "reprot_empty_direct.pdf" } else { "reprot_empty_indirect.pdf" });
        let new_pw = "newly_set_secret_password";
        let req_protect = make_request(
            "protect",
            input.to_str().unwrap(),
            reprotected.to_str().unwrap(),
            None,
            Some(new_pw),
            None,
        );
        op_protect(&req_protect, &cancel, |_| {}).expect("Re-protect on empty user password PDF must succeed");
        assert!(reprotected.exists(), "Re-protected file must exist");
        assert!(Document::load_with_password(&reprotected, new_pw).is_ok(), "Must open with new password");
        assert!(Document::load_with_password(&reprotected, "").is_err(), "Must not open with empty password anymore");

        // Verify unlocked output from re-protected PDF
        let final_unlock = temp.file(if direct { "final_unlock_empty_direct.pdf" } else { "final_unlock_empty_indirect.pdf" });
        let req_final = make_request(
            "unlock",
            reprotected.to_str().unwrap(),
            final_unlock.to_str().unwrap(),
            Some(new_pw),
            None,
            None,
        );
        op_unlock(&req_final, &cancel, |_| {}).expect("Unlock re-protected file must succeed");
        let final_plain = Document::load(&final_unlock).expect("Final unlocked file parses");
        assert_plain_document_intact(&final_plain);
    }
}
