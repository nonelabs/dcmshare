# PACS Matrix Gateway

A solution for encrypted medical image exchange between healthcare facilities using cloud storage and credential exchange via matrix-protocol 

## Overview
While DVD drives are becoming increasingly rare in modern computers and laptops, DVDs are still commonly used in clinical settings for exchanging medical imaging data between practices and clinics. This project implements a cloud-based approach to medical image sharing, leveraging the matrix messenger system instead of traditional physical media.

## Features

DICOM Gateway: Acts as a bridge between PACS and cloud storage

Cloud Storage: Compatible with S3 storage solutions (e.g., AWS S3)

Matrix Messenger: Automated notification system for image sharing and credential exchange

End-to-End Encryption: Secure data transmission and storage

PACS Compatibility: Seamless integration with existing PACS infrastructure

## How It Works

### Image Reception

DICOM Gateway receives imaging data from PACS via DICOM protocol
Data is encrypted before transmission.

### Cloud Storage

Encrypted data is uploaded to S3 cloud storage
Secure access credentials are generated

### Credential exchange

Gateway acts as a Matrix client. 
Automatically notifies designated recipients
Sends access credentials are recipient

### Data Retrieval

Receiving facility's DICOM Gateway uses access credentials to 
locate and download image data from S3 storage, decrypt and 
eventually upload the data to the recipients PACS system
