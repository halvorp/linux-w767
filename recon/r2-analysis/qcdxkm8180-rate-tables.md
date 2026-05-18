# qcdxkm8180.sys — frequency tables in .rdata

## Method
Scanned the .rdata section (PE offset 1311744+340000) as u32 LE arrays, filtered
entries to plausible clock rates (10 MHz - 1.5 GHz). Identified contiguous runs.

## Suspected interconnect / NoC rate tables (NOT dispcc PLL tables)

Tables have a typical Qualcomm shape: TURBO/NOM/SVS_L1/SVS/LOW_SVS = 5 entries.

| Offset (PE) | Rate 0 | Rate 1 | Rate 2 | Rate 3 | Rate 4 |
|---|---|---|---|---|---|
| 0x1ab7d8 | 1076.9 MHz | 1076.9 MHz | 1076.9 MHz | 1076.9 MHz | 1076.9 MHz |
| 0x1ab838 | 1076.9 MHz | 1076.9 MHz | 1076.9 MHz | 1076.9 MHz | 533.0 MHz |
| 0x1ab8d8 | 500.0 MHz | 434.0 MHz | 334.0 MHz | 270.0 MHz | 150.0 MHz |
| 0x1aba18 | 533.0 MHz | 444.0 MHz | 365.0 MHz | 338.0 MHz | 240.0 MHz |
| 0x1abbd0 | 14.8 MHz | 1076.9 MHz | 1076.9 MHz | 1076.9 MHz | 1076.9 MHz |
| 0x1abc28 | 1076.9 MHz | 1076.9 MHz | 1076.9 MHz | 1076.9 MHz | 1076.9 MHz |
| 0x1abd0c | 509.6 MHz | 509.6 MHz | 522.2 MHz | 507.4 MHz | 513.7 MHz |
| 0x1abd6c | 507.5 MHz | 515.9 MHz | 509.5 MHz | 507.5 MHz | 517.9 MHz |
| 0x1abdcc | 507.5 MHz | 511.7 MHz | 509.5 MHz | 528.5 MHz | 515.9 MHz |
| 0x1abe2c | 524.2 MHz | 515.8 MHz | 513.6 MHz | 520.1 MHz | 515.9 MHz |
| 0x1abe8c | 520.0 MHz | 515.8 MHz | 513.6 MHz | 528.4 MHz | 515.8 MHz |
| 0x1abeec | 29.3 MHz | 536.8 MHz | 524.2 MHz | 20.9 MHz | 532.6 MHz |
| 0x1abf50 | 271.1 MHz | 235.8 MHz | 521.4 MHz | 103.0 MHz | 34.1 MHz |
| 0x1abfac | 27.2 MHz | 536.8 MHz | 12.5 MHz | 52.3 MHz | 25.1 MHz |
| 0x1ac00c | 46.1 MHz | 12.6 MHz | 25.4 MHz | 39.8 MHz | 12.5 MHz |
| 0x1ac070 | 304.9 MHz | 253.0 MHz | 924.3 MHz | 472.2 MHz | 403.8 MHz |
| 0x1ac0cc | 69.2 MHz | 39.8 MHz | 16.8 MHz | 58.8 MHz | 29.5 MHz |
| 0x1ac124 | 172.9 MHz | 576.9 MHz | 543.1 MHz | 341.3 MHz | 812.3 MHz |
| 0x1ac17c | 35.9 MHz | 62.9 MHz | 35.7 MHz | 50.5 MHz | 77.7 MHz |
| 0x1ac1d4 | 1045.5 MHz | 608.4 MHz | 978.1 MHz | 877.0 MHz | 507.6 MHz |
| 0x1ac22c | 46.3 MHz | 56.9 MHz | 63.1 MHz | 42.1 MHz | 50.7 MHz |
| 0x1ac284 | 843.7 MHz | 742.7 MHz | 506.9 MHz | 775.9 MHz | 1111.8 MHz |
| 0x1ac2dc | 71.6 MHz | 52.8 MHz | 59.2 MHz | 69.4 MHz | 52.6 MHz |
| 0x1ac334 | 775.8 MHz | 1078.1 MHz | 1010.8 MHz | 641.3 MHz | 910.2 MHz |
| 0x1ac38c | 65.4 MHz | 79.9 MHz | 65.3 MHz | 50.6 MHz | 71.7 MHz |
| 0x1ac3e4 | 1010.1 MHz | 775.2 MHz | 1010.4 MHz | 943.1 MHz | 876.0 MHz |
| 0x1ac43c | 54.9 MHz | 61.2 MHz | 75.8 MHz | 61.1 MHz | 67.5 MHz |
| 0x1ac494 | 976.2 MHz | 1144.1 MHz | 942.5 MHz | 1077.0 MHz | 1043.2 MHz |
| 0x1ac4ec | 71.6 MHz | 63.3 MHz | 67.5 MHz | 69.6 MHz | 59.1 MHz |
| 0x1ac54c | 73.8 MHz | 65.4 MHz | 69.7 MHz | 524.2 MHz | 524.2 MHz |
| 0x1ac5ac | 536.8 MHz | 522.2 MHz | 524.2 MHz | 29.5 MHz | 530.8 MHz |
| 0x1ac60c | 111.2 MHz | 62.9 MHz | 21.0 MHz | 157.3 MHz | 85.9 MHz |
| 0x1ac66c | 176.1 MHz | 90.1 MHz | 25.0 MHz | 522.1 MHz | 526.3 MHz |
| 0x1ac6d0 | 1039.9 MHz | 435.7 MHz | 641.7 MHz | 139.5 MHz | 321.1 MHz |
| 0x1ac72c | 54.5 MHz | 129.9 MHz | 54.4 MHz | 528.4 MHz | 522.1 MHz |
| 0x1ac78c | 10.5 MHz | 524.4 MHz | 520.1 MHz | 134.2 MHz | 77.6 MHz |
| 0x1ac7ec | 169.9 MHz | 90.1 MHz | 27.1 MHz | 524.2 MHz | 524.2 MHz |
| 0x1ac84c | 520.2 MHz | 104.9 MHz | 50.3 MHz | 127.9 MHz | 54.4 MHz |
| 0x1ac918 | 16.5 MHz | 15.5 MHz | 14.4 MHz | 13.4 MHz | 12.3 MHz |
| 0x1ac998 | 449.6 MHz | 398.7 MHz | 331.0 MHz | 280.0 MHz | 229.0 MHz |
| 0x1ac9f0 | 144.2 MHz | 93.2 MHz | 25.5 MHz | 715.7 MHz | 614.7 MHz |
| 0x1aca48 | 480.0 MHz | 395.8 MHz | 311.5 MHz | 210.5 MHz | 109.5 MHz |
| 0x1acaa0 | 932.3 MHz | 814.8 MHz | 697.2 MHz | 596.3 MHz | 478.8 MHz |
| 0x1acaf8 | 294.0 MHz | 176.4 MHz | 58.8 MHz | 1048.6 MHz | 914.4 MHz |
| 0x1acb50 | 713.1 MHz | 578.8 MHz | 444.6 MHz | 310.4 MHz | 176.2 MHz |
| 0x1acba8 | 1298.6 MHz | 1197.9 MHz | 1097.2 MHz | 979.8 MHz | 879.1 MHz |
| 0x1acc00 | 711.4 MHz | 594.0 MHz | 476.5 MHz | 1431.7 MHz | 1331.0 MHz |
| 0x1acc58 | 1196.8 MHz | 1112.9 MHz | 1029.0 MHz | 928.3 MHz | 844.5 MHz |
| 0x1accb0 | 1246.3 MHz | 1196.0 MHz | 1162.4 MHz | 1128.9 MHz | 1095.3 MHz |
| 0x1acd08 | 1028.2 MHz | 994.7 MHz | 961.1 MHz | 1078.0 MHz | 1078.0 MHz |
| 0x1acd60 | 1078.0 MHz | 1078.0 MHz | 1078.0 MHz | 1078.0 MHz | 1078.0 MHz |
| 0x1acdec | 1073.7 MHz | 536.9 MHz | 1061.1 MHz | 1065.3 MHz | 511.5 MHz |
| 0x1ace4c | 536.9 MHz | 1061.1 MHz | 1069.5 MHz | 1071.6 MHz | 1056.8 MHz |
| 0x1ad070 | 470.3 MHz | 152.0 MHz | 117.8 MHz | 957.2 MHz | 488.4 MHz |
| 0x1ad160 | 672.1 MHz | 219.9 MHz | 185.1 MHz | 1058.6 MHz | 522.7 MHz |
| 0x1ad250 | 756.5 MHz | 254.0 MHz | 235.7 MHz | 1075.9 MHz | 540.0 MHz |
| 0x1ad340 | 807.3 MHz | 271.2 MHz | 269.4 MHz | 1026.4 MHz | 691.4 MHz |
| 0x1ad398 | 507.4 MHz | 306.2 MHz | 608.7 MHz | 676.5 MHz | 137.5 MHz |
| 0x1ad3f0 | 522.7 MHz | 690.8 MHz | 338.9 MHz | 337.6 MHz | 858.4 MHz |
| 0x1ad44c | 52.5 MHz | 61.0 MHz | 119.5 MHz | 90.2 MHz | 58.7 MHz |
| 0x1ad4a4 | 943.3 MHz | 943.3 MHz | 238.6 MHz | 708.4 MHz | 708.4 MHz |
| 0x1ad4fc | 90.3 MHz | 58.8 MHz | 59.1 MHz | 105.0 MHz | 75.6 MHz |
| 0x1ad554 | 707.4 MHz | 875.2 MHz | 1076.5 MHz | 875.2 MHz | 1076.4 MHz |
| 0x1ad5ac | 65.5 MHz | 82.1 MHz | 71.6 MHz | 71.7 MHz | 78.0 MHz |
| 0x1ad604 | 1042.9 MHz | 808.1 MHz | 975.8 MHz | 975.8 MHz | 1075.8 MHz |
| 0x1ad65c | 67.6 MHz | 67.6 MHz | 67.6 MHz | 67.6 MHz | 67.6 MHz |
| 0x1ad6b4 | 1075.8 MHz | 1075.8 MHz | 1075.8 MHz | 1075.8 MHz | 1075.8 MHz |
| 0x1ad710 | 539.0 MHz | 539.0 MHz | 1075.4 MHz | 1075.4 MHz | 1075.4 MHz |
