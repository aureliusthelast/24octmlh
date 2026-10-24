import { InvoiceData } from './types';

export const CLEAN_USD_SAMPLE: InvoiceData = {
  invoice_id: 'INV-2026-8801',
  vendor_name: 'Apex Cloud Infrastructure Ltd.',
  invoice_date: '2026-10-24',
  currency: 'USD',
  items: [
    {
      description: 'Primary GPU Cluster Hours (A100 x 8)',
      quantity: 120.0,
      unit_price: 32.5,
      total: 3900.0,
    },
    {
      description: 'High Throughput NVMe Block Storage (5TB)',
      quantity: 5.0,
      unit_price: 95.0,
      total: 475.0,
    },
    {
      description: 'Dedicated Interconnect Bandwidth (Gbps)',
      quantity: 2.0,
      unit_price: 180.0,
      total: 360.0,
    },
  ],
  subtotal: 4735.0,
  tax: 426.15,
  total_amount: 5161.15,
};

export const CLEAN_INR_SAMPLE: InvoiceData = {
  invoice_id: 'INV-INR-2026-101',
  vendor_name: 'Tata Consultancy & Cloud Ltd.',
  invoice_date: '2026-10-24',
  currency: 'INR',
  items: [
    {
      description: 'Managed Kubernetes Cluster Pods',
      quantity: 1.0,
      unit_price: 150000.0,
      total: 150000.0,
    },
    {
      description: 'High IOPS Database Volumes (NVMe)',
      quantity: 2.0,
      unit_price: 25000.0,
      total: 50000.0,
    },
  ],
  subtotal: 200000.0,
  tax: 36000.0,
  total_amount: 236000.0,
};

export const FAULTY_SAMPLE: InvoiceData = {
  invoice_id: 'INV-2026-9902-FAULTY',
  vendor_name: 'N/A', // Suspicious vendor
  invoice_date: '2026-10-24',
  currency: 'USD',
  items: [
    {
      description: 'Offshore Software Consulting Hours',
      quantity: 100.0,
      unit_price: 150.0,
      total: 15000.0, // Expected: 100 * 150 = 15000
    },
    {
      description: 'Expedited Security Penetration Audit',
      quantity: 2.0,
      unit_price: 2000.0,
      total: 4500.0, // Math error: 2 * 2000 = 4000 != 4500
    },
  ],
  subtotal: 18500.0, // Discrepancy: items total 15000 + 4500 = 19500 != 18500
  tax: 1500.0,
  total_amount: 25000.0, // Discrepancy: subtotal 18500 + tax 1500 = 20000 != 25000, and >= $10k threshold
};

export const MESSY_JSON_STRING: string = `\`\`\`json
{
  "status": "success",
  "document": {
    "invoiceId": "INV-2026-MESSY-77",
    "vendorName": "Global Cloud HyperScale Inc.",
    "invoiceDate": "2026-10-24",
    "currencyCode": "INR",
    "lineItems": [
      {
        "name": "Cloud TPU v5e Pod Slice Hours",
        "qty": "25.0",
        "unitPrice": "₹ 1,200.00",
        "amount": "₹ 30,000.00"
      },
      {
        "name": "High Performance Lustre File Storage",
        "qty": "2",
        "unitPrice": "₹ 7,500.00",
        "amount": "₹ 15,000.00"
      }
    ],
    "subTotal": "₹ 45,000.00",
    "taxAmount": "₹ 8,100.00",
    "totalAmount": "₹ 53,100.00"
  }
}
\`\`\``;
