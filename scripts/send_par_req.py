#!/usr/bin/env python3

import asyncio
import aiohttp
import os
from pathlib import Path
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)


async def process_file(session, file_path):
    """Process a single file using aiohttp."""
    filename = os.path.basename(file_path)

    try:
        # Prepare the file for upload
        data = aiohttp.FormData()
        data.add_field("file", open(file_path, "rb"), filename=filename)

        async with session.post("http://localhost:3002/parse", data=data) as response:
            if response.status == 200:
                result = await response.text()
                logger.info(f"Successfully processed: {filename}")
                return filename, result
            else:
                logger.error(f"Error processing {filename}: Status {response.status}")
                return filename, None
    except Exception as e:
        logger.error(f"Exception processing {filename}: {str(e)}")
        return filename, None


async def process_directory(input_dir, max_concurrent=4):
    """Process all PDF files in the directory with concurrency limit."""
    input_path = Path(input_dir)
    pdf_files = list(input_path.glob("*.pdf"))

    if not pdf_files:
        logger.warning(f"No PDF files found in {input_dir}")
        return

    # Create temporary directory for responses
    output_dir = Path("/tmp/pdf_responses")
    output_dir.mkdir(exist_ok=True)
    logger.info(f"Storing responses in: {output_dir}")

    # Configure connection pooling
    connector = aiohttp.TCPConnector(limit=max_concurrent)
    async with aiohttp.ClientSession(connector=connector) as session:
        # Create tasks for all files
        tasks = [process_file(session, str(pdf)) for pdf in pdf_files]

        # Process files and gather results
        results = await asyncio.gather(*tasks)

        # Save results
        for filename, content in results:
            if content:
                output_file = output_dir / f"{filename}.json"
                with open(output_file, "w") as f:
                    f.write(content)


def main():
    # Directory containing the files
    INPUT_DIR = "/Users/amine/data/quivr/parsing/native/"
    MAX_CONCURRENT = 10

    # Run the async process
    asyncio.run(process_directory(INPUT_DIR, MAX_CONCURRENT))
    logger.info("All files processed.")


if __name__ == "__main__":
    main()
