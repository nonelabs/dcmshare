use clap::Parser;
use dicom_core::{dicom_value, header::Tag, DataElement, VR};
use dicom_dictionary_std::tags;
use dicom_encoding::transfer_syntax;
use dicom_object::{mem::InMemDicomObject, open_file, StandardDataDictionary};
use dicom_transfer_syntax_registry::TransferSyntaxRegistry;
use dicom_ul::{
    association::ClientAssociationOptions,
    pdu::{PDataValue, PDataValueType, Pdu},
};
use snafu::prelude::*;
use snafu::{Report, Whatever};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, error, info, warn, Level};
use transfer_syntax::TransferSyntaxIndex;
use walkdir::WalkDir;
use snafu::prelude::*;


struct DicomFile {
    /// File path
    file: PathBuf,
    /// Storage SOP Class UID
    sop_class_uid: String,
    /// Storage SOP Instance UID
    sop_instance_uid: String,
    /// File Transfer Syntax
    file_transfer_syntax: String,
    /// Transfer Syntax selected
    ts_selected: Option<String>,
    /// Presentation Context selected
    pc_selected: Option<dicom_ul::pdu::PresentationContextResult>,
}

#[derive(Debug, Snafu)]
pub enum Error {
    /// Could not initialize SCU
    InitScu {
        source: dicom_ul::association::client::Error,
    },


    #[snafu(whatever, display("{}", message))]
    Other {
        message: String,
        #[snafu(source(from(Box<dyn std::error::Error + 'static>, Some)))]
        source: Option<Box<dyn std::error::Error + 'static>>,
    },
}


pub fn send(files: Vec<PathBuf>) -> Result<(), Error> {

    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(if true { Level::DEBUG } else { Level::INFO })
            .finish(),
    )
        .whatever_context("Could not set up global logging subscriber")
        .unwrap_or_else(|e: Whatever| {
            eprintln!("[ERROR] {}", Report::from_error(e));
        });

    let addr: String = "ORTHANC@127.0.0.1:4242".to_string();
    let mut checked_files: Vec<PathBuf> = vec![];
    let mut dicom_files: Vec<DicomFile> = vec![];
    let mut presentation_contexts = HashSet::new();

    for file in files {
        if file.is_dir() {
            for file in WalkDir::new(file.as_path())
                .into_iter()
                .filter_map(Result::ok)
                .filter(|f| !f.file_type().is_dir())
            {
                checked_files.push(file.into_path());
            }
        } else {
            checked_files.push(file);
        }
    }

    for file in checked_files {
        if true {
            info!("Opening file '{}'...", file.display());
        }

        match check_file(&file) {
            Ok(dicom_file) => {
                presentation_contexts.insert((
                    dicom_file.sop_class_uid.to_string(),
                    dicom_file.file_transfer_syntax.clone(),
                ));
                dicom_files.push(dicom_file);
            }
            Err(_) => {
                warn!("Could not open file {} as DICOM", file.display());
            }
        }
    }

    if dicom_files.is_empty() {
        eprintln!("No supported files to transfer");
        std::process::exit(-1);
    }

    if true {
        info!("Establishing association with '{}'...", &addr);
    }

    let mut scu_init = ClientAssociationOptions::new()
        .calling_ae_title("DCMSHARE")
        .max_pdu_length(16384);

    for (storage_sop_class_uid, transfer_syntax) in &presentation_contexts {
        scu_init = scu_init.with_presentation_context(storage_sop_class_uid, vec![transfer_syntax]);
    }


    let mut scu = scu_init.establish_with(&addr).context(InitScuSnafu)?;

    if true {
        info!("Association established");
    }

    for file in &mut dicom_files {
        // TODO(#106) transfer syntax conversion is currently not supported
        let r: Result<_, Error> = check_presentation_contexts(file, scu.presentation_contexts())
            .whatever_context::<_, _>("Could not choose a transfer syntax");
        match r {
            Ok((pc, ts)) => {
                file.pc_selected = Some(pc);
                file.ts_selected = Some(ts);
            }
            Err(e) => {
                error!("{}", Report::from_error(e));
            }
        }
    }


    for file in dicom_files {
        if let (Some(pc_selected), Some(ts_uid_selected)) = (file.pc_selected, file.ts_selected) {
            let cmd = store_req_command(&file.sop_class_uid, &file.sop_instance_uid, 1);

            let mut cmd_data = Vec::with_capacity(128);
            cmd.write_dataset_with_ts(
                &mut cmd_data,
                &dicom_transfer_syntax_registry::entries::IMPLICIT_VR_LITTLE_ENDIAN.erased(),
            );


            let mut object_data = Vec::with_capacity(2048);
            let dicom_file =
                open_file(&file.file).whatever_context("Could not open listed DICOM file")?;
            let ts_selected = TransferSyntaxRegistry
                .get(&ts_uid_selected)
                .whatever_context("Unsupported file transfer syntax")?;
            dicom_file
                .write_dataset_with_ts(&mut object_data, ts_selected)
                .whatever_context("Could not write object dataset")?;

            let nbytes = cmd_data.len() + object_data.len();

            if true {
                info!(
                    "Sending file {} (~ {} kB), uid={}, sop={}, ts={}",
                    file.file.display(),
                    nbytes / 1_000,
                    &file.sop_instance_uid,
                    &file.sop_class_uid,
                    ts_uid_selected,
                );
            }

            if nbytes < scu.acceptor_max_pdu_length().saturating_sub(100) as usize {
                let pdu = Pdu::PData {
                    data: vec![
                        PDataValue {
                            presentation_context_id: pc_selected.id,
                            value_type: PDataValueType::Command,
                            is_last: true,
                            data: cmd_data,
                        },
                        PDataValue {
                            presentation_context_id: pc_selected.id,
                            value_type: PDataValueType::Data,
                            is_last: true,
                            data: object_data,
                        },
                    ],
                };

                scu.send(&pdu)
                    .whatever_context("Failed to send C-STORE-RQ")?;
            } else {
                let pdu = Pdu::PData {
                    data: vec![PDataValue {
                        presentation_context_id: pc_selected.id,
                        value_type: PDataValueType::Command,
                        is_last: true,
                        data: cmd_data,
                    }],
                };

                scu.send(&pdu)
                    .whatever_context("Failed to send C-STORE-RQ command")?;

                {
                    let mut pdata = scu.send_pdata(pc_selected.id);
                    pdata
                        .write_all(&object_data)
                        .whatever_context("Failed to send C-STORE-RQ P-Data")?;
                }
            }

            if true {
                debug!("Awaiting response...");
            }

            let rsp_pdu = scu
                .receive()
                .whatever_context("Failed to receive C-STORE-RSP")?;

            match rsp_pdu {
                Pdu::PData { data } => {
                    let data_value = &data[0];

                    let cmd_obj = InMemDicomObject::read_dataset_with_ts(
                        &data_value.data[..],
                        &dicom_transfer_syntax_registry::entries::IMPLICIT_VR_LITTLE_ENDIAN
                            .erased(),
                    )
                        .whatever_context("Could not read response from SCP")?;
                    if true {
                        debug!("Full response: {:?}", cmd_obj);
                    }
                    let status = cmd_obj
                        .element(tags::STATUS)
                        .whatever_context("Could not find status code in response")?
                        .to_int::<u16>()
                        .whatever_context("Status code in response is not a valid integer")?;
                    let storage_sop_instance_uid = file
                        .sop_instance_uid
                        .trim_end_matches(|c: char| c.is_whitespace() || c == '\0');

                    match status {
                        // Success
                        0 => {
                            if true {
                                info!("Successfully stored instance {}", storage_sop_instance_uid);
                            }
                        }
                        // Warning
                        1 | 0x0107 | 0x0116 | 0xB000..=0xBFFF => {
                            warn!(
                                "Possible issue storing instance `{}` (status code {:04X}H)",
                                storage_sop_instance_uid, status
                            );
                        }
                        0xFF00 | 0xFF01 => {
                            warn!(
                                "Possible issue storing instance `{}`: status is pending (status code {:04X}H)",
                                storage_sop_instance_uid, status
                            );
                        }
                        0xFE00 => {
                            error!(
                                "Could not store instance `{}`: operation cancelled",
                                storage_sop_instance_uid
                            );
                            if false {
                                let _ = scu.abort();
                                std::process::exit(-2);
                            }
                        }
                        _ => {
                            error!(
                                "Failed to store instance `{}` (status code {:04X}H)",
                                storage_sop_instance_uid, status
                            );
                            if false {
                                let _ = scu.abort();
                                std::process::exit(-2);
                            }
                        }
                    }
                }

                pdu @ Pdu::Unknown { .. }
                | pdu @ Pdu::AssociationRQ { .. }
                | pdu @ Pdu::AssociationAC { .. }
                | pdu @ Pdu::AssociationRJ { .. }
                | pdu @ Pdu::ReleaseRQ
                | pdu @ Pdu::ReleaseRP
                | pdu @ Pdu::AbortRQ { .. } => {
                    error!("Unexpected SCP response: {:?}", pdu);
                    let _ = scu.abort();
                    std::process::exit(-2);
                }
            }
        }
    }


    scu.release()
        .whatever_context("Failed to release SCU association")?;
    Ok(())
}

fn store_req_command(
    storage_sop_class_uid: &str,
    storage_sop_instance_uid: &str,
    message_id: u16,
) -> InMemDicomObject<StandardDataDictionary> {
    InMemDicomObject::from_element_iter([
        // SOP Class UID
        DataElement::new(
            tags::AFFECTED_SOP_CLASS_UID,
            VR::UI,
            dicom_value!(Str, storage_sop_class_uid),
        ),

        // command field
        DataElement::new(
            tags::COMMAND_FIELD,
            VR::US,
            dicom_value!(U16, [0x0001]),
        ),

        // message ID
        DataElement::new(
            tags::MESSAGE_ID,
            VR::US,
            dicom_value!(U16, [1]),
        ),

        //priority
        DataElement::new(
            tags::PRIORITY,
            VR::US,
            dicom_value!(U16, [0x0000]),
        ),

        // data set type
        DataElement::new(
            tags::COMMAND_DATA_SET_TYPE,
            VR::US,
            dicom_value!(U16, [0x0000]),
        ),

        // affected SOP Instance UID
        DataElement::new(
            tags::AFFECTED_SOP_INSTANCE_UID,
            VR::UI,
            dicom_value!(Str, storage_sop_instance_uid),
        ),
    ])
}

fn check_file(file: &Path) -> Result<DicomFile, Error> {
    // Ignore DICOMDIR files until better support is added
    let _ = (file.file_name() != Some(OsStr::new("DICOMDIR")))
        .then_some(false)
        .whatever_context("DICOMDIR file not supported")?;
    let dicom_file = dicom_object::OpenFileOptions::new()
        .read_until(Tag(0x0001, 0x000))
        .open_file(file)
        .with_whatever_context(|_| format!("Could not open DICOM file {}", file.display()))?;

    let meta = dicom_file.meta();

    let storage_sop_class_uid = &meta.media_storage_sop_class_uid;
    let storage_sop_instance_uid = &meta.media_storage_sop_instance_uid;
    let transfer_syntax_uid = &meta.transfer_syntax.trim_end_matches('\0');
    let ts = TransferSyntaxRegistry
        .get(transfer_syntax_uid)
        .whatever_context("Unsupported file transfer syntax")?;
    Ok(DicomFile {
        file: file.to_path_buf(),
        sop_class_uid: storage_sop_class_uid.to_string(),
        sop_instance_uid: storage_sop_instance_uid.to_string(),
        file_transfer_syntax: String::from(ts.uid()),
        ts_selected: None,
        pc_selected: None,
    })
}

fn check_presentation_contexts(
    file: &DicomFile,
    pcs: &[dicom_ul::pdu::PresentationContextResult],
) -> Result<(dicom_ul::pdu::PresentationContextResult, String), Error> {
    let file_ts = TransferSyntaxRegistry
        .get(&file.file_transfer_syntax)
        .whatever_context("Unsupported file transfer syntax")?;
    // TODO(#106) transfer syntax conversion is currently not supported
    let pc = pcs
        .iter()
        .find(|pc| {
            // Check support for this transfer syntax.
            // If it is the same as the file, we're good.
            // Otherwise, uncompressed data set encoding
            // and native pixel data is required on both ends.
            let ts = &pc.transfer_syntax;
            ts == file_ts.uid()
                || TransferSyntaxRegistry
                .get(&pc.transfer_syntax)
                .filter(|ts| file_ts.is_codec_free() && ts.is_codec_free())
                .map(|_| true)
                .unwrap_or(false)
        })
        .whatever_context("No presentation context accepted")?;
    let ts = TransferSyntaxRegistry
        .get(&pc.transfer_syntax)
        .whatever_context("Poorly negotiated transfer syntax")?;

    Ok((pc.clone(), String::from(ts.uid())))
}
