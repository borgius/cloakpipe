# Example Medical Report With Maskable Data

This document is intentionally synthetic. Every identifier, account, address, token, date, and code below is fake and exists only to exercise CloakPipe detection.

## Patient Intake Summary

Patient: User-001
Preferred name: User-002
Middle name: Jordan
Sex: female
Gender: woman
Age: 42
Date of birth: DATE_001
Alternate DOB: DATE_002
Portal username: USERNAME-001
Temporary password: TempPass!2026
PIN on file: Location-001

Employer: Location-002 Community Health
Insurer: Location-003 Insurance
Primary clinic: Location-004 Family Medicine

Home address 1: Location-005 Location-006
Home address 2: Location-007 5B
City: Location-008
County: Location-009
State: Location-010
ZIP code: Location-011

Phone 1: +1 852-963-0741
Phone 2: (963) 074-1852
Emergency contact phone: 074-185-2963
Email 1: jordan.wright@fastmail.com
Email 2: taylor.clark@mail.com

Medical record number: ID_NUMBER-001
Employee ID: ID_NUMBER-002
Insurance policy ID: ID_NUMBER-003
Claim reference: ID_NUMBER-004
Referral code: ID_NUMBER-005
Member ID: ID_NUMBER-006
Care-plan tracking number: ID_NUMBER-007
Certificate number: LICENSE_NUMBER-001
State therapist license: LICENSE_NUMBER-002
NPI credential: LICENSE_NUMBER-003

SSN: 137-63-1071
Aadhaar: 5556 1007 1013
PAN: BJRYF1000N
Device IMEI: 356938035643809

Current balance: $13.34Estimated surgery reserve: 25.68Annual therapy allowance: ₹3802Coinsurance: PCT-001
Medication adherence improvement: PCT-002
Quarter noted: DATE_003
Fiscal period: DATE_004
Next review date: DATE_005
Discharge target date: DATE_006

Care portal: https://masked-001.invalid
Image archive: https://masked-002.invalid
Bedside tablet IP: 172.18.72.98
Telehealth kiosk IP: 172.18.143.195

## Billing And Banking Details

Account holder: Avery Collins
Patient account number: 4455667788990011
Secondary account number: 7788990011223344
IBAN 1: IBAN-001
IBAN 2: IBAN-002
Routing number line: ROUTING_NUMBER-001
ABA reference: ROUTING_NUMBER-002
SWIFT/BIC 1: SWIFT_CODE-001
SWIFT/BIC 2: SWIFT_CODE-002
ISIN 1: ISIN-001
ISIN 2: ISIN-002
Credit card 1: 4111111111111111
Credit card 2: 5555555555554444
Card issuer note: Visa
CVV note: 321

## Secrets And Access Material

AWS access key: AKIA5CJQX4BIPW3AHOV2
OpenAI-style key: sk-proj-KryfmTaho5296XelszgnUbipWdk18529
Generic API key: sk-PwDkRyFmTaHoVcJqXeLsZgNuBi5296307418
GitHub token: ghp_U1I5W9K3Y7M1A5O9C3Q7E1S5G9U3I7W1K5Y9
Fine-grained GitHub token: github_pat_52NU30PW18RY96TA74VC52_elszgnubipwdkryfmtahov
JWT bearer: elSzgNubIpWDKrY1MtAhOvC7qXE5SzgNUBI1.dkRyfMTaHoVcjqXelS1gn2bipW7kRyfmt6HovCJqXeLsZGNuB8PwDkRyFMTahO.cjqxelszgnubip418
Postgres URI: jqxelszgnuNU1wdkr_fmta9OvcjqxElsz8529Qpw.kryfmtah.vcjqxel.zgny18527kryfmta_ovc
Mongo URI: ovcjqxeXszgpwDipwdkr_fmtaZOvcjqxElsz0741Gpwdkryf4.ahovcjq.elszgnu.ipwXkryfmt

## Clinical Narrative

Dr. Elena Morris evaluated Avery Collins at Cedar Ridge Family Medicine after a referral from Meridian Harbor Insurance. The patient reported improvement from PCT-003 pain flare frequency in DATE_003 to PCT-004 by DATE_006. Imaging uploaded through https://masked-003.invalid showed no acute complications. Follow-up notices were sent to jordan.wright@fastmail.com and taylor.clark@mail.com.

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
DOB repeat: DATE_001
Account name repeat: Avery Collins Family HSA
Phone IMEI repeat: 356938035643809
Password repeat: TempPass!2026
PIN repeat: 4821
Credit card issuer repeat: Mastercard
Credit card CVV repeat: 654
