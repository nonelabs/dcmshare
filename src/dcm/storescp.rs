use sha2::{Sha256, Digest};

use sodiumoxide::crypto::secretbox;
use sodiumoxide::randombytes;

use serde::Serialize;
use serde_derive::Deserialize;

use std::{fs, net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream}, path::PathBuf};

use clap::Parser;
use dicom_core::{dicom_value, DataElement, PrimitiveValue, VR};
use dicom_dictionary_std::tags;
use dicom_dictionary_std::tags::STUDY_INSTANCE_UID;
use dicom_encoding::transfer_syntax::TransferSyntaxIndex;
use dicom_object::{FileMetaTableBuilder, InMemDicomObject, StandardDataDictionary};
use dicom_transfer_syntax_registry::TransferSyntaxRegistry;
use dicom_ul::{pdu::PDataValueType, Pdu};
use dicom_object::FileDicomObject;
use snafu::{OptionExt, ResultExt, Whatever};
use tracing::{error, info, warn, Level};
use microkv::MicroKV;
use rand::Rng;

pub static ABSTRACT_SYNTAXES: &[&str] = &[
    "1.2.840.10008.5.1.4.1.1.2",
    "1.2.840.10008.5.1.4.1.1.2.1",
    "1.2.840.10008.5.1.4.1.1.9",
    "1.2.840.10008.5.1.4.1.1.8",
    "1.2.840.10008.5.1.4.1.1.7",
    "1.2.840.10008.5.1.4.1.1.6",
    "1.2.840.10008.5.1.4.1.1.5",
    "1.2.840.10008.5.1.4.1.1.4",
    "1.2.840.10008.5.1.4.1.1.4.1",
    "1.2.840.10008.5.1.4.1.1.4.2",
    "1.2.840.10008.5.1.4.1.1.4.3",
    "1.2.840.10008.5.1.4.1.1.3",
    "1.2.840.10008.5.1.4.1.1.2",
    "1.2.840.10008.5.1.4.1.1.1",
    "1.2.840.10008.5.1.4.1.1.1.1",
    "1.2.840.10008.5.1.4.1.1.1.1.1",
    "1.2.840.10008.5.1.4.1.1.104.1",
    "1.2.840.10008.5.1.4.1.1.104.2",
    "1.2.840.10008.5.1.4.1.1.104.3",
    "1.2.840.10008.5.1.4.1.1.11.1",
    "1.2.840.10008.5.1.4.1.1.12.1",
    "1.2.840.10008.5.1.4.1.1.128",
    "1.2.840.10008.5.1.4.1.1.13.1.3",
    "1.2.840.10008.5.1.4.1.1.13.1.4",
    "1.2.840.10008.5.1.4.1.1.13.1.5",
    "1.2.840.10008.5.1.4.1.1.130",
    "1.2.840.10008.5.1.4.1.1.481.1",
    "1.2.840.10008.5.1.4.1.1.20",
    "1.2.840.10008.5.1.4.1.1.3.1",
    "1.2.840.10008.5.1.4.1.1.7",
    "1.2.840.10008.5.1.4.1.1.7.1",
    "1.2.840.10008.5.1.4.1.1.7.2",
    "1.2.840.10008.5.1.4.1.1.7.3",
    "1.2.840.10008.5.1.4.1.1.7.4",
    "1.2.840.10008.5.1.4.1.1.88.11",
    "1.2.840.10008.5.1.4.1.1.88.22",
    "1.2.840.10008.5.1.4.1.1.88.33",
];

#[derive(Serialize, Deserialize)]
#[derive(Debug)]
pub struct StoreScp {
    pub ae_title: String,
    pub run_dir: PathBuf,
}

pub fn receive(scu_stream: TcpStream, ae_title: String, run_dir: PathBuf) -> Result<String, Whatever> {

    let mut buffer: Vec<u8> = Vec::with_capacity(16384);
    let mut instance_buffer: Vec<u8> = Vec::with_capacity(1024 * 1024);
    let mut msgid = 1;
    let mut sop_class_uid = "".to_string();
    let mut sop_instance_uid = "".to_string();
    let mut options = dicom_ul::association::ServerAssociationOptions::new()
        .accept_any()
        .ae_title(ae_title);
    let mut file_path = run_dir.clone();

    for ts in TransferSyntaxRegistry.iter() {
        if !ts.unsupported() {
            options = options.with_transfer_syntax(ts.uid());
        }
    }

    for uid in ABSTRACT_SYNTAXES {
        options = options.with_abstract_syntax(*uid);
    }

    let mut association = options
        .establish(scu_stream)
        .whatever_context("could not establish association")?;

    info!("New association from {}", association.client_ae_title());
    info!(
            "> Presentation contexts: {:?}",
            association.presentation_contexts()
        );

    loop {
        match association.receive() {
            Ok(mut pdu) => {
                match pdu {
                    Pdu::PData { ref mut data } => {
                        if data.is_empty() {
                            info!("Ignoring empty PData PDU");
                            continue;
                        }

                        if data[0].value_type == PDataValueType::Data && !data[0].is_last {
                            instance_buffer.append(&mut data[0].data);
                        } else if data[0].value_type == PDataValueType::Command && data[0].is_last {
                            let ts =
                                dicom_transfer_syntax_registry::entries::IMPLICIT_VR_LITTLE_ENDIAN
                                    .erased();
                            let data_value = &data[0];
                            let v = &data_value.data;
                            let obj = InMemDicomObject::read_dataset_with_ts(v.as_slice(), &ts)
                                .whatever_context("failed to read incoming DICOM command")?;
                            msgid = obj
                                .element(tags::MESSAGE_ID)
                                .whatever_context("Missing Message ID")?
                                .to_int()
                                .whatever_context("Message ID is not an integer")?;
                            sop_class_uid = obj
                                .element(tags::AFFECTED_SOP_CLASS_UID)
                                .whatever_context("missing Affected SOP Class UID")?
                                .to_str()
                                .whatever_context("could not retrieve Affected SOP Class UID")?
                                .to_string();
                            sop_instance_uid = obj
                                .element(tags::AFFECTED_SOP_INSTANCE_UID)
                                .whatever_context("missing Affected SOP Instance UID")?
                                .to_str()
                                .whatever_context("could not retrieve Affected SOP Instance UID")?
                                .to_string();
                            instance_buffer.clear();
                        } else if data[0].value_type == PDataValueType::Data && data[0].is_last {
                            instance_buffer.append(&mut data[0].data);

                            let presentation_context = association
                                .presentation_contexts()
                                .iter()
                                .find(|pc| pc.id == data[0].presentation_context_id)
                                .whatever_context("missing presentation context")?;
                            let ts = &presentation_context.transfer_syntax;

                            let obj = InMemDicomObject::read_dataset_with_ts(
                                instance_buffer.as_slice(),
                                TransferSyntaxRegistry.get(ts).unwrap(),
                            )
                                .whatever_context("failed to read DICOM data object")?;
                            let file_meta = FileMetaTableBuilder::new()
                                .media_storage_sop_class_uid(
                                    obj.element(tags::SOP_CLASS_UID)
                                        .whatever_context("missing SOP Class UID")?
                                        .to_str()
                                        .whatever_context("could not retrieve SOP Class UID")?,
                                )
                                .media_storage_sop_instance_uid(
                                    obj.element(tags::SOP_INSTANCE_UID)
                                        .whatever_context("missing SOP Instance UID")?
                                        .to_str()
                                        .whatever_context("missing SOP Instance UID")?,
                                )
                                .transfer_syntax(ts)
                                .build()
                                .whatever_context("failed to build DICOM meta file information")?;
                            let file_obj = obj.with_exact_meta(file_meta);
                            file_path
                                .push(sop_instance_uid.trim_end_matches('\0').to_string());
                            file_obj
                                .write_to_file(&file_path)
                                .whatever_context("could not save DICOM object to file")?;
                            info!("Stored {}", file_path.display());


                            let ts =
                                dicom_transfer_syntax_registry::entries::IMPLICIT_VR_LITTLE_ENDIAN
                                    .erased();

                            let obj =
                                create_cstore_response(msgid, &sop_class_uid, &sop_instance_uid);

                            let mut obj_data = Vec::new();

                            obj.write_dataset_with_ts(&mut obj_data, &ts)
                                .whatever_context("could not write response object")?;

                            let pdu_response = Pdu::PData {
                                data: vec![dicom_ul::pdu::PDataValue {
                                    presentation_context_id: data[0].presentation_context_id,
                                    value_type: PDataValueType::Command,
                                    is_last: true,
                                    data: obj_data,
                                }],
                            };
                            association
                                .send(&pdu_response)
                                .whatever_context("failed to send response object to SCU")?;
                        }
                    }
                    Pdu::ReleaseRQ => {
                        buffer.clear();
                        association.send(&Pdu::ReleaseRP).unwrap_or_else(|e| {
                            warn!(
                                    "Failed to send association release message to SCU: {}",
                                    snafu::Report::from_error(e)
                                );
                        });
                        info!(
                                "Released association with {}",
                                association.client_ae_title()
                            );
                    }
                    _ => {}
                }
            }
            Err(err @ dicom_ul::association::server::Error::Receive { .. }) => {
                info!("{}", err);
                break;
            }
            Err(err) => {
                warn!("Unexpected error: {}", snafu::Report::from_error(err));
                break;
            }
        }
    }
    info!("Dropping connection with {}", association.client_ae_title());
    Ok(sop_instance_uid)
}

fn create_cstore_response(
    message_id: u16,
    sop_class_uid: &str,
    sop_instance_uid: &str,
) -> InMemDicomObject<StandardDataDictionary> {
    let mut obj = InMemDicomObject::new_empty();

    // group length
    obj.put(DataElement::new(
        tags::COMMAND_GROUP_LENGTH,
        VR::UL,
        PrimitiveValue::from(
            8 + sop_class_uid.len() as i32
                + 8
                + 2
                + 8
                + 2
                + 8
                + 2
                + 8
                + 2
                + sop_instance_uid.len() as i32,
        ),
    ));

    // service
    obj.put(DataElement::new(
        tags::AFFECTED_SOP_CLASS_UID,
        VR::UI,
        dicom_value!(Str, sop_class_uid),
    ));
    // command
    obj.put(DataElement::new(
        tags::COMMAND_FIELD,
        VR::US,
        dicom_value!(U16, [0x8001]),
    ));
    // message ID being responded to
    obj.put(DataElement::new(
        tags::MESSAGE_ID_BEING_RESPONDED_TO,
        VR::US,
        dicom_value!(U16, [message_id]),
    ));
    // data set type
    obj.put(DataElement::new(
        tags::COMMAND_DATA_SET_TYPE,
        VR::US,
        dicom_value!(U16, [0x0101]),
    ));

    obj.put(DataElement::new(
        tags::STATUS,
        VR::US,
        dicom_value!(U16, [0x0000]),
    ));
    // SOPInstanceUID
    obj.put(DataElement::new(
        tags::AFFECTED_SOP_INSTANCE_UID,
        VR::UI,
        dicom_value!(Str, sop_instance_uid),
    ));

    obj
}
