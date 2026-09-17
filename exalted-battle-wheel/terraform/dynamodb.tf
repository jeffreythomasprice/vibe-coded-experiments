# One definition per table, shared with `dev.sh`, which feeds the same files to
# `aws dynamodb create-table --cli-input-json`. DynamoDB is schemaless outside the key, so no
# other attribute (is_admin, log, members, ...) appears in any of these JSON files -- TTL is the
# one thing `create-table` can't carry, so it's declared here instead (and set with
# `update-time-to-live` in `dev.sh`).
locals {
  table_definitions = {
    access_codes = { file = "access-codes-table.json", ttl_attribute = null, deletion_protection = true }
    rooms        = { file = "rooms-table.json", ttl_attribute = "expires_at", deletion_protection = false }
    connections  = { file = "websocket-connections-table.json", ttl_attribute = "expires_at", deletion_protection = false }
  }
  tables = {
    for key, definition in local.table_definitions : key => merge(definition, {
      spec = jsondecode(file("${path.module}/../dynamodb/${definition.file}"))
    })
  }
}

resource "aws_dynamodb_table" "this" {
  for_each = local.tables

  name                        = each.value.spec.TableName
  billing_mode                = each.value.spec.BillingMode
  hash_key                    = one([for key in each.value.spec.KeySchema : key.AttributeName if key.KeyType == "HASH"])
  deletion_protection_enabled = each.value.deletion_protection

  dynamic "attribute" {
    for_each = each.value.spec.AttributeDefinitions
    content {
      name = attribute.value.AttributeName
      type = attribute.value.AttributeType
    }
  }

  dynamic "ttl" {
    for_each = each.value.ttl_attribute == null ? [] : [each.value.ttl_attribute]
    content {
      attribute_name = ttl.value
      enabled        = true
    }
  }
}

# The access-codes table used to be its own named resource, back when it was the only table this
# config managed -- this keeps `terraform apply` from reading that rename as "destroy the live
# table, create a new one," which would delete every access code in production.
moved {
  from = aws_dynamodb_table.access_codes
  to   = aws_dynamodb_table.this["access_codes"]
}
