import sys
import requests
import os

# --- Configuration & Constants (hardcoded) ---
GITHUB_API_URL = "https://api.github.com"
OPENAI_API_URL = "https://api.openai.com/v1/chat/completions"
MODEL_NAME = "o3-mini"
MIN_DIFF_SIZE = 75

# Our base prompt without the diff.
BASE_PROMPT = (
    "You are a seasoned code reviewer. Please analyze the following cumulative code diff - for a TauriV2/React19 time tracking application - and provide a strong but to the point review for the PR. Only comment on changes directly introduced in the diff — ignore unrelated assumptions or suggestions or hallucinations. Follow these instructions regarding content perfectly, do not hallucinate and ensure that you are following the directions as a whole since they apply to each section. Format your response in Markdown with the following structure:\n\n # PR Code Review Analysis\n\n ## Summary:\n Concisely summarize the changes introduced in the diff. No additional comments. No points about adding todos. No points about changing ENVs. No points about double checking. \n\n ## Changes:\n To the point bullet points listing only functional code changes. Ignore formatting, styling, test updates, or unrelated improvements. Write the file name after the period of each bullet point.\n\n ## Detailed Observations:\n Bullet points listing only functional issues or potential bugs directly introduced in the diff. No generic suggestions (like check accessibility or verify behavior). No points about changing styles/tailwind classes. No points about adding todos. No points about changing ENVs. No points about double checking. No points about changing regex and constants/text changes or updates. \n\n ## Fixes and Improvements:\n Bullet points listing actionable recommendations to take care of any fixes. We are using React19 and TailwindV4 and TauriV2/Rust with a Node/Express backend. Only include specific, value-adding improvements or corrections related to core functionalities that appear in the diff. Write the file name for each bullet point at the end in parentheses. No points about changing regex and constants/text changes. No points about adding todos. Do not mention testing, accessibility, or behavioral verification unless clearly broken in the code diff itself. Do not mention fixes or improvements that contain action to: Verify or Double Check or Confirm That or Check Constants etc. Rate bug fixes, 3 or 5 or 8 or 10. 10 being a super critical - a fix-now urgent bug.\n Lastly, make the markdown fun but proffessional with seperate icons exactly as follows to make it a little more enjoyable to read, they should be in front of the line item.. Icon for New Features:trophy:. Icon for Updates of/on Existing Features:stars:. Icon for Removals:skull:. Icon for Fixes:rocket:. Icon for Refactoring:wrench:"
)

# Retrieve configuration from environment variables or hardcoded for local testing
OWNER = os.getenv("OWNER")
REPO = os.getenv("REPO")
PR_NUMBER = os.getenv("PR_NUMBER")

# Retrieve tokens from environment variables or hardcoded for local testing
GITHUB_TOKEN = os.getenv("GITHUB_TOKEN")
OPENAI_API_KEY = os.getenv("OPENAI_API_KEY")

# --- Helper Functions ---

def get_changed_files(OWNER: str, repo: str, pr_number: int, github_token: str):
    """
    Retrieve the list of changed files (with patch/diff data) in a PR.
    """
    url = f"{GITHUB_API_URL}/repos/{OWNER}/{REPO}/pulls/{int(PR_NUMBER)}/files"
    headers = {
        "Authorization": f"Bearer {github_token}",
        "Accept": "application/vnd.github.v3+json",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    response = requests.get(url, headers=headers)
    response.raise_for_status()
    return response.json()

def calculate_diff_size(diff_text: str):
    """
    Calculate the size of the diff by counting added and removed characters.
    """
    added_count = diff_text.count("\n+")
    removed_count = diff_text.count("\n-")
    return added_count + removed_count

def call_openai_api(prompt: str, openai_api_key: str):
    """
    Call OpenAI's API with the provided prompt and return the response text.
    """
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {openai_api_key}"
    }
    body = {
        "model": MODEL_NAME,
        "messages": [
            {"role": "user", "content": prompt}
        ]
    }
    response = requests.post(OPENAI_API_URL, headers=headers, json=body)
    response.raise_for_status()
    data = response.json()
    return data["choices"][0]["message"]["content"]

def get_existing_comments():
    """
    Fetch all comments for a given PR to check if a previous review comment exists.
    """
    url = f"{GITHUB_API_URL}/repos/{OWNER}/{REPO}/issues/{PR_NUMBER}/comments"
    headers = {
        "Authorization": f"Bearer {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    response = requests.get(url, headers=headers)
    response.raise_for_status()
    return response.json()

def find_existing_pr_review_comment(comments):
    """
    Check for an existing PR review comment that contains the PR Code Review Analysis markdown header.
    Returns the comment ID if found, otherwise returns None.
    """
    for comment in comments:
        if "Detailed Observations:" in comment["body"]:
            return comment["id"]
    return None

def delete_existing_comment(comment_id):
    """
    Delete an existing PR review comment with the given comment ID.
    """
    url = f"{GITHUB_API_URL}/repos/{OWNER}/{REPO}/issues/comments/{comment_id}"
    headers = {
        "Authorization": f"Bearer {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    response = requests.delete(url, headers=headers)
    response.raise_for_status()
    print(f"Existing comment {comment_id} deleted.")

def post_comment(review: str):
    """
    Post a new comment with the PR review analysis.
    """
    url = f"{GITHUB_API_URL}/repos/{OWNER}/{REPO}/issues/{PR_NUMBER}/comments"
    comment = {"body": review}
    response = requests.post(url, headers={
        "Authorization": f"Bearer {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json",
        "X-GitHub-Api-Version": "2022-11-28"
    }, json=comment)
    response.raise_for_status()

def save_review_to_file(filename: str, content: str):
    """
    Save the review content to a text file.
    """
    with open(filename, "w", encoding="utf-8") as f:
        f.write(content)
    print(f"Saved review to {filename}")

def main():
    """
    Main function to perform the PR review process.
    """
    print(f"Fetching changed files for PR #{PR_NUMBER} in {OWNER}/{REPO}...")
    files = get_changed_files(OWNER, REPO, int(PR_NUMBER), GITHUB_TOKEN)

    # Extensions to ignore
    ignore_extensions = (
        '.yml', '.css', '.json', '.lock', '.env', '.txt', 
        '.png', '.jpg', '.jpeg', '.gif', '.svg', '.ico', 
        '.ttf', '.woff', '.woff2', '.eot', '.otf', '.webp', '.md', '.htm', '.xml', '.jsonld', '.csv', '.yaml', '.yml', '.toml'
    )

    aggregated_diff = ""
    total_diff_size = 0

    for file_info in files:
        filename = file_info.get("filename")
        patch = file_info.get("patch")

        # Skip if there's no patch (e.g. binary file).
        if not patch:
            print(f"Skipping {filename} (no patch available)")
            continue

        # Skip if the file extension is in the ignored list.
        if filename.lower().endswith(ignore_extensions):
            print(f"Skipping {filename} (ignored file extension)")
            continue

        # Append a header for each file to clearly separate diffs.
        aggregated_diff += f"\n### File: {filename}\n{patch}\n"
        total_diff_size += calculate_diff_size(patch) #Accumulate total diff size

    if not aggregated_diff.strip():
        print("No applicable diffs found to review.")
        return

    if total_diff_size < MIN_DIFF_SIZE:
        print(f"Total diff size ({total_diff_size}) is less than {MIN_DIFF_SIZE}. Skipping AI review.")
        return

    # Create the prompt for the entire PR
    prompt = BASE_PROMPT + aggregated_diff
    print("Sending aggregated diff to OpenAI API...")
    try:
        review_response = call_openai_api(prompt, OPENAI_API_KEY)
    except Exception as e:
        print(f"Error calling OpenAI API: {e}")
        return

    output_filename = "pr_review_summary.txt"
    save_review_to_file(output_filename, review_response)

    # Fetch the list of comments and check if an existing review comment exists
    comments = get_existing_comments()
    existing_comment_id = find_existing_pr_review_comment(comments)

    if existing_comment_id:
        print(f"Found existing PR review comment {existing_comment_id}. Deleting it...")
        delete_existing_comment(existing_comment_id)

    print("Posting review comment...")
    post_comment(review_response)

    if os.path.exists(output_filename):
        print("Logging AI PR Review...")
        with open(output_filename, "r") as f:
            print(f.read())
    else:
        print("No AI PR Review found.")

if __name__ == "__main__":
    main()
