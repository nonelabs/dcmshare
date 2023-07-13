use std::fs;
use dicom_core::{dicom_value, DataElement, PrimitiveValue, VR, Tag};
use dicom_dictionary_std::tags;
use dicom_dictionary_std::tags::{STUDY_INSTANCE_UID, STUDY_DATE, PATIENT_ID, PATIENT_NAME, PATIENT_BIRTH_DATE, PATIENT_BIRTH_NAME};
use dicom_encoding::transfer_syntax::TransferSyntaxIndex;
use dicom_object::{DefaultDicomObject, FileMetaTableBuilder, InMemDicomObject, StandardDataDictionary};
use dicom_transfer_syntax_registry::TransferSyntaxRegistry;
use dicom_ul::{pdu::PDataValueType, Pdu};
use dicom_object::FileDicomObject;
use sodiumoxide::crypto::secretbox;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::str::FromStr;
use dicom_object::open_file;
use microkv::MicroKV;
use snafu::{ResultExt, Whatever};
use sha2::{Sha256, Digest};
use uuid::Uuid;
use std::path::Path;
use dicom_encoding::DataRWAdapter;
use s3::{Bucket, Region};
use s3::creds::Credentials;
use s3::error::S3Error;
use tokio::runtime::Runtime;

pub struct StudyStore {
    pub run_dir: PathBuf,
    pub db: MicroKV,
}

#[derive(Debug)]
pub struct Study {
    pub study_instance_uid:String,
    pub study_instance_uid_hash:String,
    pub study_date:String,
    pub patient_id:String,
    pub patient_name:String,
    pub patient_birth_date:String,
    pub new_study: bool,
    pub hex_key: String,
}

impl Study {
    fn new(db: &MicroKV) -> StudyBuilder {
        StudyBuilder::new(db.clone())
        }
    }


struct StudyBuilder {
    db: MicroKV,
    study_instance_uid: Option<String>,
    study_instance_uid_hash: Option<String>,
    study_date:String,
    patient_id:String,
    patient_name:String,
    patient_birth_date:String,
    hex_key : Option<String>,
    new_study: bool
}

impl StudyBuilder {

    pub fn new(db: MicroKV) -> StudyBuilder {
        StudyBuilder {
            db: db,
            study_instance_uid:  None,
            study_instance_uid_hash: None,
            study_date: "N/A".to_string(),
            patient_id: "N/A".to_string(),
            patient_name: "N/A".to_string(),
            patient_birth_date: "N/A".to_string(),
            hex_key: None,
            new_study: false
        }
    }

    fn study_instance_uid(mut self, uid: String) -> StudyBuilder {
        self.study_instance_uid = Some(uid);
        let hex_key: String = match self.db.exists(self.study_instance_uid.clone().unwrap()) {
            Ok(v) => {
                if !v {
                    let key = secretbox::gen_key();
                    let hex_key = hex::encode(&key);
                    self.new_study = true;
                    hex_key
                } else {
                    self.db.get_unwrap(self.study_instance_uid.clone().unwrap()).unwrap()
                }
            },
            Err(v) => {
                println!("Something went wrong !");
                stringify!("").to_string()
            }
            ,
        };
        self.hex_key = Some(hex_key);
        let mut hasher = Sha256::new();
        hasher.update(format!("{}_{}", self.hex_key.as_ref().unwrap(), &self.study_instance_uid.as_ref().unwrap()));
        self.study_instance_uid_hash = Some(format!("{:x}",hasher.finalize()));
        self
    }

    fn study_date(mut self, study_date: String) -> StudyBuilder {
        self.study_date = study_date;
        self
    }
    fn patient_id(mut self, patient_id: String) -> StudyBuilder {
        self.patient_id = patient_id;
        self
    }
    fn patient_name(mut self, patient_name: String) -> StudyBuilder {
        self.patient_name = patient_name;
        self
    }
    fn patient_birth_date(mut self, patient_birth_date: String) -> StudyBuilder {
        self.patient_birth_date = patient_birth_date;
        self
    }
    fn from_dicom_object(self,obj: &DefaultDicomObject) -> StudyBuilder {
        let study_instance_uid = get_tag(&obj,STUDY_INSTANCE_UID);
        let study_date = get_tag(&obj,STUDY_DATE);
        let patient_id = get_tag(&obj,PATIENT_ID);
        let patient_name = get_tag(&obj,PATIENT_NAME);
        let patient_birth_data = get_tag(&obj,PATIENT_BIRTH_DATE);
        self.study_instance_uid(study_instance_uid)
        .study_date(study_date)
        .patient_id(patient_id)
        .patient_name(patient_name)
        .patient_birth_date(patient_birth_data)
    }
    fn from_dicom_file(self,dicom_file: &PathBuf) -> StudyBuilder {
        let obj = open_file(&dicom_file).unwrap();
        self.from_dicom_object(&obj)
    }

    fn build(self) -> Study {
        if self.new_study {
            self.db.put(self.study_instance_uid.as_ref().unwrap(), self.hex_key.as_ref().unwrap());
        }
        Study {
            study_instance_uid: self.study_instance_uid.unwrap().to_string(),
            study_instance_uid_hash: self.study_instance_uid_hash.unwrap(),
            study_date: self.study_date,
            patient_id: self.patient_id,
            patient_name: self.patient_name,
            patient_birth_date: self.patient_birth_date,
            hex_key: self.hex_key.unwrap(),
            new_study: self.new_study
        }
    }
}

fn get_tag(obj: &DefaultDicomObject, tag: Tag) -> String {
    obj.element(tag)
        .unwrap()
        .to_str()
        .unwrap().to_string()
}

impl StudyStore {

async fn upload_series(self,file_path: &PathBuf, object_name: &String) -> Result<(), S3Error>{
        let bucket = Bucket::new_with_path_style("studies",Region::Custom {region:"".to_owned(),endpoint: "https://nonelabs.com:9090".to_owned()}, Credentials{
            access_key: Some("minio".to_owned()),
            secret_key: Some("f381hf1h2g70hvq23ubgiu123r".to_owned()),
            expiration: None,
            security_token: None,
            session_token: None,
        });
        let file = fs::read(Path::new(&file_path))?;
        match bucket.unwrap().put_object(&object_name, file.as_slice()).await{
            Ok(T) => println!("file uploaded"),
            Err(E) => println!("Something {}",E)
        }
        println!("File uploaded successfully.");
        Ok(())
    }

pub async fn add_series(self, sop_instance_uid: &String) -> Result<Study, Whatever> {
        sodiumoxide::init();
        let mut dicom_file = self.run_dir.clone();
        dicom_file.push(&sop_instance_uid.trim_end_matches('\0').to_string());
        let study = Study::new(&self.db).from_dicom_file(&dicom_file).build();
        let key_bytes = hex::decode(&study.hex_key).expect("Invalid hex string");
        let key = secretbox::Key::from_slice(&key_bytes).expect("Invalid key");
        let mut encrypted_file = self.run_dir.clone();
        let mut hasher = Sha256::new();
        hasher.update(format!("{}_{}", &study.hex_key, &sop_instance_uid.trim_end_matches('\0').to_string()));
        let file_id = format!("{:x}",hasher.finalize());
        encrypted_file.push(PathBuf::from(&file_id));
        let mut input_file = File::open(&dicom_file).unwrap();
        let mut input_data = Vec::new();
        input_file.read_to_end(&mut input_data).unwrap();
        let nonce = secretbox::gen_nonce();
        let encrypted_data = secretbox::seal(&input_data, &nonce, &key);
        let mut output_file = File::create(&encrypted_file).unwrap();
        output_file.write_all(&nonce.0).unwrap();
        output_file.write_all(&encrypted_data);
        let object_name = format!("{}/{}",&study.study_instance_uid_hash,&file_id);
        self.upload_series(&encrypted_file, &object_name).await;
        fs::remove_file(encrypted_file);
        fs::remove_file(dicom_file);
        Ok(study)
    }
}

// pub fn decrypt(input_path: &str, output_path: &str, key: &secretbox::Key) -> Result<(), Box<dyn std::error::Error>> {
//     sodiumoxide::init();
//     let mut input_file = File::open(input_path)?;
//     let mut nonce_encrypted_data = Vec::new();
//     input_file.read_to_end(&mut nonce_encrypted_data)?;
//     let (nonce_bytes, encrypted_data) = nonce_encrypted_data.split_at(secretbox::NONCEBYTES);
//     let nonce = secretbox::Nonce::from_slice(nonce_bytes).ok_or("Failed to read nonce")?;
//     let decrypted_data = secretbox::open(&encrypted_data, &nonce, &key);
//     match decrypted_data{
//         Ok(T) => {
//             let mut output_file = File::create(output_path)?;
//             output_file.write_all(&T);
//         },
//         Err(E) => println!("Cannot decrypt"),
//
//     }
//     Ok(())
// }