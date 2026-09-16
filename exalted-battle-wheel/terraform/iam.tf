data "aws_iam_policy_document" "server_dynamodb" {
  statement {
    actions   = ["dynamodb:GetItem", "dynamodb:PutItem", "dynamodb:UpdateItem", "dynamodb:DeleteItem", "dynamodb:Scan"]
    resources = [for table in aws_dynamodb_table.this : table.arn]
  }
}

# A static key, not the instance role: the k0s node's IMDS hop limit is 1 (see
# ../kubernetes-host/terraform/instance.tf), deliberately so pods can't reach it. The key is
# exported as a sensitive output and lands in the Kubernetes Secret `deploy.sh` creates -- see
# CLAUDE.md's "Server hosting" section.
resource "aws_iam_user" "server" {
  name = "exalted-battle-wheel-server"
}

resource "aws_iam_user_policy" "server_dynamodb" {
  name   = "dynamodb"
  user   = aws_iam_user.server.name
  policy = data.aws_iam_policy_document.server_dynamodb.json
}

# Renamed along with the policy document above (it covered only access codes before rooms and
# connections existed) -- keeps the rename from reading as "delete this policy, create a new one."
moved {
  from = aws_iam_user_policy.server_access_codes
  to   = aws_iam_user_policy.server_dynamodb
}

resource "aws_iam_access_key" "server" {
  user = aws_iam_user.server.name
}
