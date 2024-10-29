# PACS Matrix Gateway

A matrix-protocol based solution for secure medical image sharing between healthcare facilities using cloud storage and matrix messenger integration.
## Overview
While DVD drives are becoming increasingly rare in modern computers and laptops, DVDs are still commonly used in clinical settings for exchanging medical imaging data between practices and clinics. This project implements a cloud-based approach to medical image sharing, leveraging the matrix messenger system instead of traditional physical media.

## Features

DICOM Gateway Integration: Acts as a bridge between PACS and cloud storage
Cloud Storage Support: Compatible with S3 storage solutions (e.g., AWS S3)
Matrix Messenger Integration: Automated notification system for image sharing
End-to-End Encryption: Secure data transmission and storage
PACS Compatibility: Seamless integration with existing PACS infrastructure

## How It Works

### Image Reception

DICOM Gateway receives imaging data from PACS via DICOM protocol
Data is encrypted before transmission

### Cloud Storage

Encrypted data is uploaded to S3 cloud storage
Secure access credentials are generated

### Credential exchange

Gateway acts as a Matrix client. 
Automatically notifies designated recipients (e.g., medical technical assistants)
Access credentials are forwarded 

### Data Retrieval

Receiving facility's DICOM Gateway uses access credentials
Data is located and downloaded from S3 storage
Automatic decryption and image upload
