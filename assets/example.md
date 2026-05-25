# Example Medical Report With Maskable Data

This document is intentionally synthetic. Every identifier, account, address, token, date, and code below is fake and exists only to exercise CloakPipe detection.

## Patient Intake Summary

Patient: Avery Collins
Preferred name: Avery
Middle name: Jordan
Sex: female
Gender: woman
Age: 42
Date of birth: 1984-07-16
Alternate DOB: July 16, 1984
Portal username: avery.collins42
Temporary password: TempPass!2026
PIN on file: 4821

Employer: Northwind Community Health
Insurer: Meridian Harbor Insurance
Primary clinic: Cedar Ridge Family Medicine

Home address 1: 1842 Willow Creek Drive
Home address 2: Apt 5B
City: Fairview
County: Jefferson
State: Oregon
ZIP code: 97035

Phone 1: +1 206-555-0184
Phone 2: (415) 555-0198
Emergency contact phone: 212-555-0176
Email 1: avery.collins@example-health.test
Email 2: intake.avery+followup@samplemail.test

Medical record number: MRN-2026-443821
Employee ID: EMP-2026-88314
Insurance policy ID: POL-2026-55219
Claim reference: CLM-2026-99102
Referral code: REF-2026-77114
Member ID: MBR-2026-22854
Care-plan tracking number: TN-2026-11493
Certificate number: CRC-1330841
State therapist license: #OR-48291
NPI credential: NPI 1184729934

SSN: 927-83-6041
Aadhaar: 2345 6789 1234
PAN: BNZPM2501F
Device IMEI: 356938035643809

Current balance: $12,480.77
Estimated surgery reserve: USD 18,500.00
Annual therapy allowance: INR 18,00,000
Coinsurance: 20%
Medication adherence improvement: 93.5%
Quarter noted: Q3 2026
Fiscal period: FY2025
Next review date: 03/14/2026
Discharge target date: March 31, 2026

Care portal: https://care.internal.example.org/patients/avery-collins
Image archive: https://pacs.example-hospital.test/studies/2026/04/avery-collins
Bedside tablet IP: 10.24.18.42
Telehealth kiosk IP: 172.16.4.9

## Billing And Banking Details

Account holder: Avery Collins
Patient account number: 4455667788990011
Secondary account number: 7788990011223344
IBAN 1: GB82WEST12345698765432
IBAN 2: DE89370400440532013000
Routing number line: Routing No. 021000021
ABA reference: ABA 026009593
SWIFT/BIC 1: DEUTDEFF500
SWIFT/BIC 2: BOFAUS3NXXX
ISIN 1: US0378331005
ISIN 2: GB0002634946
Credit card 1: 4111111111111111
Credit card 2: 5555555555554444
Card issuer note: Visa
CVV note: 321

## Secrets And Access Material

AWS access key: AKIAQX4BIPW3AHOV29GN
OpenAI-style key: sk-proj-AlphaBeta2026MaskingDemoKey77777
Generic API key: sk-AbCdEfGhIjKlMnOpQrStUvWxYz1234567890
GitHub token: ghp_A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6Q7R8
Fine-grained GitHub token: github_pat_11AA22BB33CC44DD55EE66_ffgghhiijjkkllmmnnoopp
JWT bearer: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhdmVyeS1jb2xsaW5zIiwic2NvcGUiOiJtZWRpY2FsOnJlYWQifQ.signaturetoken123
Postgres URI: postgresql://care_user:MaskedPass2026@db.internal.example.org:5432/patient_ops
Mongo URI: mongodb+srv://triage_user:MaskedPass2026@cluster0.example.mongodb.net/intake

## Clinical Narrative

Dr. Elena Morris evaluated Avery Collins at Cedar Ridge Family Medicine after a referral from Meridian Harbor Insurance. The patient reported improvement from 12% pain flare frequency in Q3 2026 to 4.5% by March 31, 2026. Imaging uploaded through https://care.internal.example.org showed no acute complications. Follow-up notices were sent to avery.collins@example-health.test and intake.avery+followup@samplemail.test.

## NER-Focused Extras

These lines target entity types the app supports when NER and phone detection are enabled.

First name: Avery
Last name: Collins
Middle name repeat: Jordan
Company name repeat: Northwind Community Health
Street repeat: 1842 Willow Creek Drive
Secondary address repeat: Apt 5B
City repeat: Fairview
State repeat: Oregon
County repeat: Jefferson
ZIP repeat: 97035
Username repeat: avery.collins42
DOB repeat: 1984-07-16
Account name repeat: Avery Collins Family HSA
Phone IMEI repeat: 356938035643809
Password repeat: TempPass!2026
PIN repeat: 4821
Credit card issuer repeat: Mastercard
Credit card CVV repeat: 654
