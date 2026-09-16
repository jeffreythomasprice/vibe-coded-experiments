# The one definition of this table, shared with `dev.sh`, which feeds the same file to
# `aws dynamodb create-table --cli-input-json`. DynamoDB is schemaless outside the key, so
# `is_admin` and `created_at` appear in neither.
locals {
  access_codes = jsondecode(file("${path.module}/../dynamodb/access-codes-table.json"))
}

resource "aws_dynamodb_table" "access_codes" {
  name         = local.access_codes.TableName
  billing_mode = local.access_codes.BillingMode
  hash_key     = one([for key in local.access_codes.KeySchema : key.AttributeName if key.KeyType == "HASH"])

  dynamic "attribute" {
    for_each = local.access_codes.AttributeDefinitions
    content {
      name = attribute.value.AttributeName
      type = attribute.value.AttributeType
    }
  }
}
